//! The live link to FMOD Studio over this process's stdin/stdout.
//!
//! When the Studio script starts the app with `--bridge`, it reads our stdout
//! and writes to our stdin on a ~15 ms timer (`cplLinkPoll`). One JSON object
//! per line each way:
//!
//!   app → Studio:  {"id": 1, "cmd": "snapshot", "args": {...}}
//!   Studio → app:  {"id": 1, "ok": true, "result": ...} or {"ok": false, "error": "..."}
//!
//! Nothing else may be printed to stdout in bridge mode; logging goes to stderr.
//!
//! **Every line read is acknowledged with an empty line, at once, from the
//! reader thread.** Studio's `writeStandardInput(text, timeout)` writes a
//! reply and then waits up to `timeout` for the process to print something;
//! without the ack, each reply froze Studio for the whole timeout (1 s in
//! script 0.5, logged as "Timeout waiting for process output"). Studio skips
//! empty lines.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A reply that takes longer than this means the link window was closed.
const TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Debug, PartialEq)]
pub enum LinkState {
    Connecting,
    Linked { studio: String, project: String },
    Lost(String),
}

enum Incoming {
    Line(String),
    Closed,
}

/// Something the app asked for, carried with the request so the reply can be
/// acted on.
#[derive(Clone, Debug)]
pub enum Pending {
    Hello,
    Snapshot,
    Apply(ApplyCtx),
    /// Undoing an apply: a re-apply of the previous points, or `removeCurve`.
    Undo(ApplyCtx),
    /// Studio's 2.04 curve presets, to import into this library.
    Presets { lib: std::path::PathBuf },
    /// Curves sent to Studio's curve presets, into this preset folder.
    WritePresets { count: usize, folder: String },
}

#[derive(Clone, Debug)]
pub struct ApplyCtx {
    pub curve_name: String,
    pub target_label: String,
    pub property: String,
    pub param_name: String,
    pub target_id: String,
    pub property_name: String,
    pub param_id: String,
    /// The curve that was there before (None: the apply created it).
    pub previous: Option<Vec<crate::studio::RawPoint>>,
}

type Out = Arc<Mutex<Box<dyn Write + Send>>>;

pub struct Bridge {
    rx: Receiver<Incoming>,
    out: Out,
    next_id: u64,
    pending: HashMap<u64, (Pending, Instant)>,
    pub state: LinkState,
}

impl Bridge {
    /// Starts reading stdin on a thread. `wake` is called after each line, to
    /// repaint the UI.
    pub fn stdio(wake: impl Fn() + Send + 'static) -> Bridge {
        let (tx, rx) = channel();
        let out: Out = Arc::new(Mutex::new(Box::new(std::io::stdout())));
        let ack = out.clone();
        std::thread::spawn(move || read_lines(std::io::stdin().lock(), tx, wake, ack));
        Bridge::with(rx, out)
    }

    fn with(rx: Receiver<Incoming>, out: Out) -> Bridge {
        Bridge { rx, out, next_id: 1, pending: HashMap::new(), state: LinkState::Connecting }
    }

    /// A linked bridge whose output is captured, for app tests.
    #[cfg(test)]
    pub fn captured() -> (Bridge, Arc<Mutex<Vec<u8>>>) {
        #[derive(Clone)]
        struct Cap(Arc<Mutex<Vec<u8>>>);
        impl Write for Cap {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let buf = Arc::new(Mutex::new(Vec::new()));
        let (_tx, rx) = channel();
        let mut b = Bridge::with(rx, Arc::new(Mutex::new(Box::new(Cap(buf.clone())))));
        b.state = LinkState::Linked { studio: "2.3.14".into(), project: "/x/p.fspro".into() };
        (b, buf)
    }

    pub fn is_linked(&self) -> bool {
        matches!(self.state, LinkState::Linked { .. })
    }

    pub fn is_waiting(&self, f: impl Fn(&Pending) -> bool) -> bool {
        self.pending.values().any(|(p, _)| f(p))
    }

    /// Sends a request. Returns false when the link is lost.
    pub fn request(&mut self, cmd: &str, args: Value, what: Pending) -> bool {
        if matches!(self.state, LinkState::Lost(_)) {
            return false;
        }
        let id = self.next_id;
        self.next_id += 1;
        let line = json!({ "id": id, "cmd": cmd, "args": args }).to_string();
        let written = match self.out.lock() {
            Ok(mut o) => writeln!(o, "{line}").and_then(|_| o.flush()),
            Err(_) => Err(std::io::Error::other("stdout lock poisoned")),
        };
        if written.is_err() {
            self.state = LinkState::Lost("Studio closed the link".into());
            return false;
        }
        self.pending.insert(id, (what, Instant::now()));
        true
    }

    /// Replies that arrived since the last call, with what they answer.
    pub fn poll(&mut self) -> Vec<(Pending, Result<Value, String>)> {
        let mut out = Vec::new();
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Incoming::Closed => {
                    self.state = LinkState::Lost("Studio closed the link (the link window or Studio was closed)".into());
                    self.pending.clear();
                }
                Incoming::Line(line) => {
                    let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                    let Some(id) = v.get("id").and_then(Value::as_u64) else { continue };
                    let Some((what, _)) = self.pending.remove(&id) else { continue };
                    let result = if v.get("ok").and_then(Value::as_bool) == Some(true) {
                        Ok(v.get("result").cloned().unwrap_or(Value::Null))
                    } else {
                        Err(v.get("error").and_then(Value::as_str).unwrap_or("Studio reported an error").to_string())
                    };
                    if let (Pending::Hello, Ok(r)) = (&what, &result) {
                        self.state = LinkState::Linked {
                            studio: r.get("studio").and_then(Value::as_str).unwrap_or("?").into(),
                            project: r.get("project").and_then(Value::as_str).unwrap_or("").into(),
                        };
                    }
                    out.push((what, result));
                }
            }
        }
        if self.pending.values().any(|(_, t)| t.elapsed() > TIMEOUT) {
            self.pending.clear();
            self.state = LinkState::Lost(
                "Studio stopped answering. Is the \"Curve Library link\" window still open? Reopen the editor from Studio's Scripts menu.".into(),
            );
        }
        out
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
    use std::sync::{Arc, Mutex};

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

    fn bridge() -> (Bridge, Sender<Incoming>, Sink) {
        let (tx, rx) = channel();
        let sink = Sink::default();
        (Bridge::with(rx, Arc::new(Mutex::new(Box::new(sink.clone())))), tx, sink)
    }

    #[test]
    fn hello_links_and_replies_match_requests() {
        let (mut b, tx, sink) = bridge();
        assert!(b.request("hello", json!({}), Pending::Hello));
        assert!(b.request("snapshot", json!({}), Pending::Snapshot));
        let sent = String::from_utf8(sink.0.lock().unwrap().clone()).unwrap();
        assert_eq!(sent.lines().count(), 2);
        assert!(sent.lines().next().unwrap().contains("\"cmd\":\"hello\""));
        // Replies out of order, plus noise.
        tx.send(Incoming::Line(r#"{"id":2,"ok":true,"result":{"sharedEffects":[]}}"#.into())).unwrap();
        tx.send(Incoming::Line("not json".into())).unwrap();
        tx.send(Incoming::Line(r#"{"id":1,"ok":true,"result":{"studio":"2.3.14","project":"/p/x.fspro"}}"#.into())).unwrap();
        let got = b.poll();
        assert_eq!(got.len(), 2);
        assert!(matches!(got[0].0, Pending::Snapshot));
        assert_eq!(b.state, LinkState::Linked { studio: "2.3.14".into(), project: "/p/x.fspro".into() });
    }

    #[test]
    fn errors_and_closing() {
        let (mut b, tx, _) = bridge();
        b.request("snapshot", json!({}), Pending::Snapshot);
        tx.send(Incoming::Line(r#"{"id":1,"ok":false,"error":"boom"}"#.into())).unwrap();
        assert_eq!(b.poll()[0].1, Err("boom".to_string()));
        tx.send(Incoming::Closed).unwrap();
        b.poll();
        assert!(matches!(b.state, LinkState::Lost(_)));
        assert!(!b.request("snapshot", json!({}), Pending::Snapshot));
    }

    #[test]
    fn read_lines_acks_every_line_and_ends_with_closed() {
        let (tx, rx) = channel();
        let sink = Sink::default();
        read_lines(std::io::Cursor::new("{\"id\":1}\n{\"id\":2}\n"), tx, || {}, Arc::new(Mutex::new(Box::new(sink.clone()))));
        let all: Vec<Incoming> = rx.try_iter().collect();
        assert_eq!(all.len(), 3);
        assert!(matches!(all[2], Incoming::Closed));
        assert_eq!(sink.0.lock().unwrap().as_slice(), b"\n\n", "one empty line per reply read");
    }
}
