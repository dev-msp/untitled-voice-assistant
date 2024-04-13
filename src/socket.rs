use std::{
    io::{BufRead, BufReader, Read, Write},
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
    channel::{unbounded, Receiver, Sender},
};
use serde_json::Value;

struct DebugBufReader<R: BufRead>(R);

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

fn write_thread(
    mut wstream: UnixStream,
    r: &Receiver<Value>,
    is_done: Arc<AtomicCell<bool>>,
) -> thread::JoinHandle<Result<(), anyhow::Error>> {
    let r = r.clone();
    thread::spawn(move || {
        while !is_done.load() {
            if let Ok(line) = r.recv() {
                log::trace!("Writing line: {}", line);
                wstream.write_all(format!("{line}\n").as_bytes())?;
                log::trace!("Wrote line");
            } else {
                log::error!("Failed to read line");
            }
        }
        log::debug!("Exiting write thread");
        Ok(())
    })
}

fn read_thread(
    stream: UnixStream,
    s: Sender<Value>,
    is_done: Arc<AtomicCell<bool>>,
) -> thread::JoinHandle<Result<(), anyhow::Error>> {
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
            log::trace!("Sending line: {}", line);
            s.send(line)?;
        }

        is_done.store(true);
        log::debug!("Exiting read thread");
        Ok(())
    })
}

#[tracing::instrument(skip_all)]
fn handle_stream(
    stream: UnixStream,
    cmd_send: Sender<Value>,
    res_recv: &Receiver<Value>,
) -> Result<(), anyhow::Error> {
    let wstream = stream.try_clone()?;
    log::trace!("Cloned stream");

    let is_done = Arc::new(AtomicCell::new(false));

    let reads = read_thread(stream, cmd_send, is_done.clone());
    let writes = write_thread(wstream, res_recv, is_done);

    let w_outcome = writes.join().expect("Failed to join write thread");
    let r_outcome = reads.join().expect("Failed to join read thread");

    w_outcome.unwrap();
    r_outcome.unwrap();

    log::debug!("Exiting handle_stream");
    Ok(())
}

type Handle = std::thread::JoinHandle<Result<(), anyhow::Error>>;

pub fn receive_instructions(
    socket_path: &str,
) -> Result<((Receiver<Value>, Sender<Value>), Sender<Value>, Handle), anyhow::Error> {
    match std::fs::metadata(socket_path) {
        Ok(metadata) if metadata.file_type().is_socket() => {
            std::fs::remove_file(socket_path)?;
        }
        Ok(_) => return Err(anyhow!("socket path exists and is not a Unix socket")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(anyhow!(e).context("unhandled error attempting to access socket")),
    }
    let (csend, crecv) = unbounded();
    let (rsend, rrecv) = unbounded();
    let sock_path = socket_path.to_string();
    Ok((
        (crecv, csend.clone()),
        rsend,
        std::thread::spawn(move || {
            let listener = UnixListener::bind(sock_path).expect("Failed to bind to socket");

            let mut incoming = listener.incoming();
            while let Some(rstream) = incoming.next().transpose()? {
                handle_stream(rstream, csend.clone(), &rrecv)?;
            }
            log::warn!("Listener done providing streams");
            Ok(())
        }),
    ))
}
