//! One running copy at a time. A second launch (for example from a magnet
//! link in the browser) hands its arguments to the first and exits.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::time::Duration;

const PORT: u16 = 47_823;

pub enum Incoming {
    Show,
    Open(String),
}

/// Returns a listener if we're the first instance, None if we forwarded.
pub fn claim(args: &[String]) -> Option<TcpListener> {
    match TcpListener::bind(("127.0.0.1", PORT)) {
        Ok(l) => Some(l),
        Err(_) => {
            if let Ok(mut s) = TcpStream::connect_timeout(&([127, 0, 0, 1], PORT).into(), Duration::from_secs(2)) {
                let _ = writeln!(s, "LIFESTREAM");
                if args.is_empty() {
                    let _ = writeln!(s, "SHOW");
                }
                for a in args {
                    let _ = writeln!(s, "OPEN {a}");
                }
                None
            } else {
                // Port taken by something else; run anyway.
                Some(TcpListener::bind(("127.0.0.1", 0)).ok()?)
            }
        }
    }
}

pub fn serve(listener: TcpListener, tx: Sender<Incoming>, wake: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
            let mut lines = BufReader::new(stream).lines();
            if lines.next().and_then(|l| l.ok()).as_deref() != Some("LIFESTREAM") {
                continue;
            }
            for line in lines.map_while(Result::ok) {
                if line == "SHOW" {
                    let _ = tx.send(Incoming::Show);
                } else if let Some(arg) = line.strip_prefix("OPEN ") {
                    let _ = tx.send(Incoming::Open(arg.to_string()));
                }
            }
            wake();
        }
    });
}
