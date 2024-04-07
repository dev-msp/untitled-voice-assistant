use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::FileTypeExt,
        net::{UnixListener, UnixStream},
    },
    thread::{self},
};

use anyhow::anyhow;
use crossbeam::channel::{bounded, unbounded, Receiver, Sender};

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

#[derive(Debug)]
struct State {
    half_closed: bool,
    outstanding_messages: i16,
}

#[derive(Debug)]
enum StateChange {
    Close,
    MessageRead,
    MessageWritten,
}

impl State {
    fn new() -> Self {
        Self {
            half_closed: false,
            outstanding_messages: 0,
        }
    }

    fn change(&mut self, change: StateChange) {
        match change {
            StateChange::Close => {
                self.half_closed = true;
            }
            StateChange::MessageRead => {
                self.outstanding_messages += 1;
            }
            StateChange::MessageWritten => {
                self.outstanding_messages -= 1;
            }
        }
    }

    fn ready(&self) -> bool {
        self.half_closed && self.outstanding_messages == 0
    }
}

fn write_thread(
    mut wstream: UnixStream,
    r: Receiver<String>,
    queue: Sender<StateChange>,
) -> thread::JoinHandle<Result<(), anyhow::Error>> {
    thread::spawn(move || {
        for line in r.iter() {
            log::trace!("Writing line: {}", line);
            wstream.write_all(format!("{line}\n").as_bytes())?;
            log::trace!("Wrote line");
            match queue.send(StateChange::MessageWritten) {
                Ok(_) => {}
                Err(_) => {
                    log::debug!("Failed to send message from socket write thread");
                }
            };
        }
        log::debug!("Exiting write thread");
        match queue.send(StateChange::Close) {
            Ok(_) => {}
            Err(_) => {
                log::debug!("Failed to send close from socket write thread");
            }
        }
        Ok(())
    })
}

fn read_thread(
    stream: UnixStream,
    s: Sender<String>,
    queue: Sender<StateChange>,
) -> thread::JoinHandle<Result<(), anyhow::Error>> {
    thread::spawn(move || {
        let reader = DebugBufReader(BufReader::new(stream));
        let it = reader.lines().map(|l| {
            log::trace!("Read line: {:?}", l);
            l.expect("Failed to read line")
        });
        for line in it {
            log::trace!("Sending line: {}", line);
            s.send(line)?;
            match queue.send(StateChange::MessageRead) {
                Ok(_) => {}
                Err(_) => {
                    log::warn!("Failed to send message from socket read thread");
                }
            };
        }
        log::debug!("Exiting read thread");
        match queue.send(StateChange::Close) {
            Ok(_) => {}
            Err(_) => {
                log::warn!("Failed to send close from socket read thread");
            }
        };
        Ok(())
    })
}

#[tracing::instrument(skip_all)]
fn handle_stream(
    stream: UnixStream,
    cmd_send: Sender<String>,
    res_recv: &Receiver<String>,
) -> Result<(), anyhow::Error> {
    let wstream = stream.try_clone()?;
    log::debug!("Cloned stream");

    let (s, r): (Sender<String>, Receiver<String>) = unbounded();

    let (state_s, state_r) = bounded(1);
    let reads = read_thread(stream, cmd_send, state_s.clone());
    let writes = write_thread(wstream, r, state_s);

    let (ns, nr) = crossbeam::channel::unbounded();
    let notification = thread::spawn(move || {
        if state_r
            .iter()
            .scan(State::new(), |state, change| {
                log::trace!("Received state change: {:?}", change);
                state.change(change);
                log::trace!("State: {:?}", state);
                Some(state.ready())
            })
            .any(|ready| ready)
        {
            Ok(ns.try_send(())?)
        } else {
            anyhow::bail!("No ready state");
        }
    });

    loop {
        // let Ok(response) = res_recv.recv() else {
        //     log::debug!("Failed to receive response");
        //     break;
        // };
        // log::debug!("Received response from channel: {}", response);
        //
        // s.send(response).expect("Failed to send response");
        // log::debug!("Sent response");
        crossbeam::select! {
            recv(res_recv) -> msg => {
                let response = msg?;
                log::trace!("Received response from channel: {}", response);
                s.send(response).expect("Failed to send response");
                log::trace!("Sent response");
            },
            recv(nr) -> _ => {
                log::debug!("Exiting handle_stream");
                break;
            }
        }
    }
    // So the writes thread can end
    drop(s);

    notification
        .join()
        .expect("Failed to join notification thread")?;
    let w_outcome = writes.join().expect("Failed to join write thread");
    let r_outcome = reads.join().expect("Failed to join read thread");

    w_outcome?;
    r_outcome?;

    log::debug!("Exiting handle_stream");
    Ok(())
}

type Handle = std::thread::JoinHandle<Result<(), anyhow::Error>>;

pub fn receive_instructions(
    socket_path: &str,
) -> Result<(Receiver<String>, Sender<String>, Handle), anyhow::Error> {
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
        crecv,
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
