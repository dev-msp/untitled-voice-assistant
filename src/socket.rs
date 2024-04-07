use std::{
    io::{BufRead, BufReader, Write},
    os::unix::{
        fs::FileTypeExt,
        net::{UnixListener, UnixStream},
    },
    sync::mpsc::{Receiver, Sender},
    thread::{self},
};

use anyhow::anyhow;

pub struct SocketLineIterator {
    reader: BufReader<UnixStream>,
}

impl Iterator for SocketLineIterator {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = String::new();
        match self.reader.read_line(&mut buf) {
            Ok(0) => None, // End of stream
            Ok(_) => Some(buf),
            Err(_) => None, // Error reading line
        }
    }
}

fn handle_stream(
    stream: UnixStream,
    cmd_send: Sender<String>,
    res_recv: &Receiver<String>,
) -> Result<(), anyhow::Error> {
    let mut wstream = stream.try_clone()?;

    let (s, r): (Sender<String>, Receiver<String>) = std::sync::mpsc::channel();
    let writes = thread::spawn(move || {
        for line in r.iter() {
            wstream
                .write_all(line.as_bytes())
                .expect("Failed to write to socket");
        }
    });

    let reads = thread::spawn(move || {
        let reader = BufReader::new(stream);
        let it = SocketLineIterator { reader };
        for line in it {
            cmd_send.send(line).expect("Failed to send command");
        }
    });

    loop {
        let Ok(response) = res_recv.recv() else {
            eprintln!("Failed to receive response");
            break;
        };

        s.send(response).expect("Failed to send response");
    }
    // So the writes thread can end
    drop(s);

    writes.join().expect("Failed to join write thread");
    reads.join().expect("Failed to join read thread");

    Ok(())
}

pub fn receive_instructions(
    socket_path: &str,
) -> Result<
    (
        Receiver<String>,
        Sender<String>,
        std::thread::JoinHandle<Result<(), anyhow::Error>>,
    ),
    anyhow::Error,
> {
    match std::fs::metadata(socket_path) {
        Ok(metadata) if metadata.file_type().is_socket() => {
            std::fs::remove_file(socket_path)?;
        }
        Ok(_) => return Err(anyhow!("socket path exists and is not a Unix socket")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(anyhow!(e).context("unhandled error attempting to access socket")),
    }
    let (csend, crecv) = std::sync::mpsc::channel();
    let (rsend, rrecv) = std::sync::mpsc::channel();
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
            Ok(())
        }),
    ))
}
