use std::{
    io::{BufRead, BufReader},
    os::unix::{
        fs::FileTypeExt,
        net::{UnixListener, UnixStream},
    },
    sync::mpsc::{Receiver, Sender},
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

fn sock(socket_path: &str, send: Sender<String>) {
    let listener = UnixListener::bind(socket_path).expect("Failed to bind to socket");
    let it = listener.incoming().map(|s| s.unwrap()).flat_map(|s| {
        let reader = BufReader::new(s);
        SocketLineIterator { reader }
    });

    for line in it {
        let should_break = &line == "quit";
        match send.send(line) {
            Ok(_) => {
                if should_break {
                    break;
                }
            }
            Err(line) => {
                eprintln!("Failed to send line: {}", line);
                break;
            }
        };
    }
}

pub fn receive_instructions(
    socket_path: &str,
) -> Result<(Receiver<String>, std::thread::JoinHandle<()>), anyhow::Error> {
    match std::fs::metadata(socket_path) {
        Ok(metadata) if metadata.file_type().is_socket() => {
            std::fs::remove_file(socket_path)?;
        }
        Ok(_) => return Err(anyhow!("socket path exists and is not a Unix socket")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(anyhow!(e).context("unhandled error attempting to access socket")),
    }
    let (send, recv) = std::sync::mpsc::channel();
    let sock_path = socket_path.to_string();
    Ok((
        recv,
        std::thread::spawn(move || {
            sock(&sock_path, send);
        }),
    ))
}
