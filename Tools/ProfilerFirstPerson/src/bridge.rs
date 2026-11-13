//! The live link to FMOD Studio over this process's stdin/stdout.
//!
//! The Studio script (`Scripts/DAT-audio Utilities/Open Profiler First Person View.js`) starts the app
//! with `--bridge` and, on a ~15 ms timer, pushes the followed profiler session and its
//! playhead as JSON lines (see link.rs). The app asks for event spatial data:
//! `{"cmd":"spatial","ids":[..]}`. Nothing else may be printed to
//! stdout in bridge mode; logging goes to stderr.
//!
//! **Every line read is acknowledged with an empty line, at once, from the
//! reader thread.** Studio's `writeStandardInput(text, timeout)` writes and
//! then waits for the process to print something, blocking Studio's UI until
//! it does. Studio skips empty lines.

use std::io::{BufRead, Write};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

pub enum Incoming {
    Line(String),
    Closed,
}

type Out = Arc<Mutex<Box<dyn Write + Send>>>;

pub struct Bridge {
    rx: Receiver<Incoming>,
    out: Out,
    pub closed: bool,
}

impl Bridge {
    /// Starts reading stdin on a thread. `wake` is called after each line, to
    /// repaint the UI.
    pub fn stdio(wake: impl Fn() + Send + 'static) -> Bridge {
        let (tx, rx) = channel();
        let out: Out = Arc::new(Mutex::new(Box::new(std::io::stdout())));
        // Tell the script we're up: it holds its first write until then (a
        // write before we read stdin would wait out its timeout).
        if let Ok(mut o) = out.lock() {
            let _ = writeln!(o, "{{\"type\":\"ready\"}}").and_then(|_| o.flush());
        }
        let ack = out.clone();
        std::thread::spawn(move || read_lines(std::io::stdin().lock(), tx, wake, ack));
        Bridge { rx, out, closed: false }
    }

    /// Lines that arrived since the last call.
    pub fn poll(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Incoming::Line(l) => lines.push(l),
                Incoming::Closed => self.closed = true,
            }
        }
        lines
    }

    pub fn send(&mut self, line: &str) {
        if self.closed {
            return;
        }
        let ok = match self.out.lock() {
            Ok(mut o) => writeln!(o, "{line}").and_then(|_| o.flush()).is_ok(),
            Err(_) => false,
        };
        if !ok {
            self.closed = true;
        }
    }
}

fn read_lines(input: impl BufRead, tx: Sender<Incoming>, wake: impl Fn(), ack: Out) {
    for line in input.lines() {
        let Ok(line) = line else { break };
        // Ack first, so Studio's wait in writeStandardInput ends now.
        if let Ok(mut o) = ack.lock() {
            let _ = o.write_all(b"\n").and_then(|_| o.flush());
        }
        if tx.send(Incoming::Line(line)).is_err() {
            return;
        }
        wake();
    }
    let _ = tx.send(Incoming::Closed);
    wake();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl Write for Sink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn read_lines_acks_every_line_and_ends_with_closed() {
        let (tx, rx) = channel();
        let sink = Sink::default();
        read_lines(std::io::Cursor::new("{\"type\":\"hello\"}\n{\"type\":\"listener\"}\n"), tx, || {}, Arc::new(Mutex::new(Box::new(sink.clone()))));
        let mut b = Bridge { rx, out: Arc::new(Mutex::new(Box::new(sink.clone()))), closed: false };
        assert_eq!(b.poll().len(), 2);
        assert!(b.closed);
        assert_eq!(sink.0.lock().unwrap().as_slice(), b"\n\n", "one empty line per line read");
    }
}
