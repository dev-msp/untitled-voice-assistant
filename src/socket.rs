use std::{
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::{
        fs::FileTypeExt,
        net::{UnixListener, UnixStream},
    },
    sync::Arc,
    thread::{self},
};

use anyhow::anyhow;
use crossbeam::{
    atomic::AtomicCell,
    channel::{unbounded, Receiver, SendError, Sender},
};
use serde::{de::DeserializeOwned, Serialize};

struct DebugBufReader<R: BufRead>(R);

#[derive(Debug, thiserror::Error)]
#[error("socket error: {0}")]
enum Error {
    Stream(#[from] std::io::Error),
    Read(#[from] ReadError),
    Write(#[from] WriteError),
}

#[derive(Debug, thiserror::Error)]
#[error("read: {0}")]
enum ReadError {
    #[error("parse: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("channel send failed")]
    Send,
}

#[derive(Debug, thiserror::Error)]
#[error("write error: {0}")]
enum WriteError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

impl Error {
    fn is_broken_pipe(&self) -> bool {
        match self {
            Error::Write(WriteError::Io(e)) => e.kind() == io::ErrorKind::BrokenPipe,
            _ => false,
        }
    }

    fn recoverable(&self) -> bool {
        if self.is_broken_pipe() {
            return true;
        }

        matches!(self, Error::Read(ReadError::Parse(_)))
    }
}

impl<R: BufRead> Read for DebugBufReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.0.read(buf)?;
        Ok(n)
    }
}

impl<R: BufRead> BufRead for DebugBufReader<R> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        let buf = self.0.fill_buf()?;
        log::trace!("Read bytes: {:?}", buf);
        Ok(buf)
    }

    fn consume(&mut self, amt: usize) {
        self.0.consume(amt);
    }
}

fn write_thread<T: Send + Serialize + 'static>(
    mut wstream: UnixStream,
    r: &Receiver<T>,
    is_done: Arc<AtomicCell<bool>>,
) -> thread::JoinHandle<Result<(), WriteError>> {
    let r = r.clone();
    thread::spawn(move || {
        while !is_done.load() {
            if let Ok(line) = r.recv() {
                let line = serde_json::to_string(&line)?;
                log::trace!("Writing line: {}", line);
                wstream.write_all(format!("{},\n", line).as_bytes())?;
                log::trace!("Wrote line");
            } else {
                log::error!("Failed to read line");
            }
        }
        log::debug!("Exiting write thread");
        Ok(())
    })
}

fn read_thread<T: Send + Sync + DeserializeOwned + 'static>(
    stream: UnixStream,
    s: Sender<T>,
    is_done: Arc<AtomicCell<bool>>,
) -> thread::JoinHandle<Result<(), ReadError>> {
    thread::spawn(move || {
        let reader = DebugBufReader(BufReader::new(stream));
        let it = reader.lines().flat_map(|l| {
            log::trace!("Read line: {:?}", l);
            let l = l.expect("Failed to read line");
            match serde_json::from_str(&l) {
                Ok(v) => Some(v),
                Err(_) => {
                    log::warn!("Failed to parse line: {:?}", l);
                    None
                }
            }
        });
        for line in it {
            s.send(line).map_err(|_: SendError<T>| ReadError::Send)?;
        }

        is_done.store(true);
        log::debug!("Exiting read thread");
        Ok(())
    })
}

struct ThreadName<S: ToString>(Option<S>);

impl<S: ToString> ThreadName<S> {
    fn new(name: Option<S>) -> Self {
        Self(name)
    }

    fn realize(self) -> String {
        self.0
            .map(|n| format!("thread named {}", n.to_string()))
            .unwrap_or_else(|| "thread".to_string())
    }
}

/// Opinionated function to handle thread join results
///
/// When the thread cannot be joined, this function will panic.
fn settle_thread<T, E>(
    handle: thread::JoinHandle<Result<T, E>>,
    name: Option<&'static str>,
) -> Result<T, Error>
where
    E: Into<Error>,
{
    let join_result = handle.join().unwrap_or_else(|_| {
        panic!("Failed to join {} thread", ThreadName::new(name).realize());
    });

    match join_result.map_err(Into::into) {
        Err(e) if e.recoverable() => {
            log::warn!(
                "Recoverable failure in {}: {:?}",
                name.map(|n| format!("{} thread", n))
                    .unwrap_or_else(|| "thread".to_string()),
                e
            );
            Err(e)
        }
        x => x,
    }
}

#[tracing::instrument(skip_all)]
fn handle_stream<I, O>(
    stream: UnixStream,
    cmd_send: Sender<I>,
    res_recv: &Receiver<O>,
) -> Result<(), Error>
where
    I: Send + Sync + DeserializeOwned + 'static,
    O: Send + Serialize + 'static,
{
    let wstream = stream.try_clone()?;
    log::trace!("Cloned stream");

    let is_done = Arc::new(AtomicCell::new(false));

    let reads = read_thread(stream, cmd_send, is_done.clone());
    let writes = write_thread(wstream, res_recv, is_done);

    settle_thread(writes, Some("write"))?;
    settle_thread(reads, Some("read"))?;

    log::debug!("Exiting handle_stream");
    Ok(())
}

type Handle = std::thread::JoinHandle<Result<(), anyhow::Error>>;

// Triple of channel pair (commands), sender (responses), and handle for the socket thread
type InstructionHandle<I, O> = (Receiver<I>, Sender<O>, Handle);

pub fn receive_instructions<I, O>(
    socket_path: String,
) -> Result<InstructionHandle<I, O>, anyhow::Error>
where
    I: Send + Sync + DeserializeOwned + 'static,
    O: Send + Serialize + 'static,
{
    match std::fs::metadata(&socket_path) {
        Ok(metadata) if metadata.file_type().is_socket() => {
            std::fs::remove_file(&socket_path)?;
        }
        Ok(_) => return Err(anyhow!("socket path exists and is not a Unix socket")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(anyhow!(e).context("unhandled error attempting to access socket")),
    }
    let (csend, crecv) = unbounded();
    let (rsend, rrecv) = unbounded();
    Ok((
        crecv,
        rsend,
        std::thread::spawn(move || {
            let listener = UnixListener::bind(socket_path).expect("Failed to bind to socket");

            let mut incoming = listener.incoming();
            while let Some(rstream) = incoming.next().transpose()? {
                match handle_stream(rstream, csend.clone(), &rrecv) {
                    Err(e) if e.recoverable() => Ok(()),
                    x => x,
                }?
            }
            log::warn!("Listener done providing streams");
            Ok(())
        }),
    ))
}
