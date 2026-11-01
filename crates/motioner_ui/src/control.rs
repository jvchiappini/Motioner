//! The live control endpoint of a running editor (`preview --control`) and
//! its client (`motioner ctl`), D-0024.
//!
//! The server listens on `127.0.0.1` only, on a port the OS picks, and
//! writes the port, a random token and the process id to an endpoint file.
//! A background thread accepts connections and hands each request to the UI
//! thread through a channel that `update` drains without blocking; the UI
//! runs it through [`crate::Editor::execute`] (the same path as a mouse or
//! keyboard action) and sends one JSON line back.
//!
//! Wire format, one request per connection:
//!
//! ```text
//! client -> server   <token>\n<request>      (then the client closes its write side)
//! server -> client   {"ok":true,"request":"seek(30)","result":"frame 30"}\n
//! ```
//!
//! A request is any [`Command`] in its text form, or one of the queries
//! `state()`, `query(node=None)` and `screenshot(path)`.

use std::collections::hash_map::RandomState;
use std::fmt::Write as _;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::command::{parse_calls, Args, Command, CommandError, ErrorKind};
use crate::json::Json;

/// Largest request accepted, in bytes.
const MAX_REQUEST: u64 = 64 * 1024;
/// How long a connection waits for the UI to answer.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);
/// How long the server waits for a client to send its request.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// What a control client can ask.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Run a command.
    Command(Command),
    /// The editor state (`state()`).
    State,
    /// The scene at the playhead (`query()` or `query(node="id")`).
    Query {
        /// Only this identity.
        node: Option<String>,
    },
    /// Capture the window to a PNG (`screenshot("out.png")`).
    Screenshot(PathBuf),
}

impl Request {
    /// The query names, with their signatures (for help text).
    pub const QUERIES: [&'static str; 3] = ["state()", "query(node=None)", "screenshot(path)"];

    /// Parses one request.
    ///
    /// # Errors
    ///
    /// A parse, unknown-command or argument error; more than one call is a
    /// [`ErrorKind::Parse`] error.
    pub fn parse(text: &str) -> Result<Self, CommandError> {
        let mut calls = parse_calls(text)?;
        if calls.len() != 1 {
            return Err(CommandError::new(
                ErrorKind::Parse,
                "send exactly one request per connection",
            ));
        }
        let call = calls.remove(0);
        let request = match call.name.as_str() {
            "state" => {
                Args::with_signature("state()", call.into_args()).finish()?;
                Self::State
            }
            "query" => {
                let mut args = Args::with_signature("query(node=None)", call.into_args());
                let node = args.optional_string("node")?;
                args.finish()?;
                Self::Query { node }
            }
            "screenshot" => {
                let mut args = Args::with_signature("screenshot(path)", call.into_args());
                let path = args.string("path")?;
                args.finish()?;
                Self::Screenshot(PathBuf::from(path))
            }
            _ => Self::Command(Command::from_call(call)?),
        };
        Ok(request)
    }
}

/// Where a running editor listens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// TCP port on 127.0.0.1.
    pub port: u16,
    /// Secret the client must send first.
    pub token: String,
    /// Process id of the editor.
    pub pid: u32,
}

impl Endpoint {
    /// The endpoint file's text.
    #[must_use]
    pub fn to_text(&self) -> String {
        format!(
            "port={}\ntoken={}\npid={}\n",
            self.port, self.token, self.pid
        )
    }

    /// Reads an endpoint file.
    ///
    /// # Errors
    ///
    /// An I/O error, or `InvalidData` for a malformed file.
    pub fn read(path: &Path) -> io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let field = |name: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
                .map(str::trim)
                .ok_or_else(|| invalid(format!("{} has no '{name}'", path.display())))
        };
        Ok(Self {
            port: field("port")?
                .parse()
                .map_err(|_| invalid("bad port".to_owned()))?,
            token: field("token")?.to_owned(),
            pid: field("pid")?
                .parse()
                .map_err(|_| invalid("bad pid".to_owned()))?,
        })
    }
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// The endpoint file used when none is given: `<temp>/motioner/control.txt`
/// (the most recently started editor).
#[must_use]
pub fn default_endpoint_file() -> PathBuf {
    std::env::temp_dir().join("motioner").join("control.txt")
}

/// A random 128-bit token in hex. `RandomState` is seeded from the OS per
/// process; mixing in the time and pid makes two editors differ even with
/// the same seed.
fn random_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut out = String::new();
    for salt in 0..2_u64 {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u128(nanos);
        hasher.write_u32(std::process::id());
        hasher.write_u64(salt);
        let _ = write!(out, "{:016x}", hasher.finish());
    }
    out
}

/// A request waiting for the UI, with the way back to its client.
#[derive(Debug)]
pub struct Pending {
    /// The parsed request (or why it did not parse).
    pub request: Result<Request, CommandError>,
    text: String,
    reply: Sender<String>,
}

impl Pending {
    fn send(&self, ok: bool, write: impl FnOnce(&mut Json)) {
        let mut j = Json::new();
        j.begin_object();
        j.key("ok").boolean(ok);
        j.key("request").string(&self.text);
        write(&mut j);
        j.end_object();
        let _ = self.reply.send(j.finish());
    }

    /// Answers with a one-line summary.
    pub fn reply_ok_summary(&self, summary: &str) {
        self.send(true, |j| {
            j.key("result").string(summary);
        });
    }

    /// Answers with a JSON value.
    pub fn reply_ok_json(&self, json: &str) {
        self.send(true, |j| {
            j.key("result").raw(json);
        });
    }

    /// Answers with a typed error.
    pub fn reply_error(&self, error: &CommandError) {
        self.send(false, |j| {
            j.key("error").begin_object();
            j.key("kind").string(error.kind.name());
            j.key("message").string(&error.message);
            j.end_object();
        });
    }

    /// Answers a screenshot request.
    pub fn reply_screenshot(&self, path: &Path, width: u32, height: u32) {
        self.send(true, |j| {
            j.key("result").begin_object();
            j.key("path").string(&path.display().to_string());
            j.key("width").int(i64::from(width));
            j.key("height").int(i64::from(height));
            j.end_object();
        });
    }

    /// The target file of a screenshot request.
    #[must_use]
    pub fn screenshot_path(&self) -> Option<&PathBuf> {
        match &self.request {
            Ok(Request::Screenshot(path)) => Some(path),
            _ => None,
        }
    }
}

/// The server side, owned by the UI thread.
#[derive(Debug)]
pub struct ControlServer {
    requests: Receiver<Pending>,
    endpoint: Endpoint,
    file: PathBuf,
}

impl ControlServer {
    /// Binds `127.0.0.1:0`, writes the endpoint file and starts accepting.
    ///
    /// # Errors
    ///
    /// When the socket cannot be bound or the endpoint file written.
    pub fn start(file: &Path) -> io::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))?;
        let endpoint = Endpoint {
            port: listener.local_addr()?.port(),
            token: random_token(),
            pid: std::process::id(),
        };
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::write_atomic(file, &endpoint.to_text())?;
        let (sender, requests) = mpsc::channel();
        let token = endpoint.token.clone();
        thread::Builder::new()
            .name("motioner-control".to_owned())
            .spawn(move || accept_loop(&listener, &token, &sender))?;
        Ok(Self {
            requests,
            endpoint,
            file: file.to_owned(),
        })
    }

    /// The endpoint clients connect to.
    #[must_use]
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Requests that arrived since the last call (never blocks).
    #[must_use]
    pub fn poll(&self) -> Vec<Pending> {
        self.requests.try_iter().collect()
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        // Only remove the file if it still names this editor.
        if Endpoint::read(&self.file).is_ok_and(|e| e == self.endpoint) {
            let _ = std::fs::remove_file(&self.file);
        }
    }
}

fn accept_loop(listener: &TcpListener, token: &str, sender: &Sender<Pending>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let token = token.to_owned();
        let sender = sender.clone();
        let _ = thread::Builder::new()
            .name("motioner-control-client".to_owned())
            .spawn(move || {
                let _ = serve(stream, &token, &sender);
            });
    }
}

fn serve(mut stream: TcpStream, token: &str, sender: &Sender<Pending>) -> io::Result<()> {
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    let mut text = String::new();
    (&mut stream).take(MAX_REQUEST).read_to_string(&mut text)?;
    let (sent_token, request) = text.split_once('\n').unwrap_or((text.as_str(), ""));
    if sent_token.trim() != token {
        stream.write_all(
            b"{\"ok\":false,\"error\":{\"kind\":\"unauthorized\",\"message\":\"bad token\"}}\n",
        )?;
        return Ok(());
    }
    let request_text = request.trim().to_owned();
    let (reply, answer) = mpsc::channel();
    let pending = Pending {
        request: Request::parse(&request_text),
        text: request_text,
        reply,
    };
    let line = if sender.send(pending).is_ok() {
        answer.recv_timeout(REPLY_TIMEOUT).unwrap_or_else(|_| {
            "{\"ok\":false,\"error\":{\"kind\":\"timeout\",\"message\":\"the editor did not answer\"}}"
                .to_owned()
        })
    } else {
        "{\"ok\":false,\"error\":{\"kind\":\"closed\",\"message\":\"the editor is closing\"}}"
            .to_owned()
    };
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()
}

/// Sends one request to a running editor and returns its JSON reply line.
///
/// # Errors
///
/// When the editor cannot be reached or does not answer in time.
pub fn send(endpoint: &Endpoint, request: &str) -> io::Result<String> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, endpoint.port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
    stream.set_read_timeout(Some(REPLY_TIMEOUT + Duration::from_secs(5)))?;
    stream.write_all(endpoint.token.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.write_all(request.as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply)?;
    Ok(reply.trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_parse_queries_and_commands() {
        assert_eq!(Request::parse("state()").unwrap(), Request::State);
        assert_eq!(
            Request::parse("query(node=\"title\")").unwrap(),
            Request::Query {
                node: Some("title".to_owned())
            }
        );
        assert_eq!(
            Request::parse("query()").unwrap(),
            Request::Query { node: None }
        );
        assert_eq!(
            Request::parse("screenshot(\"a.png\")").unwrap(),
            Request::Screenshot(PathBuf::from("a.png"))
        );
        assert_eq!(
            Request::parse("seek(3)").unwrap(),
            Request::Command(Command::Seek(3))
        );
        assert_eq!(
            Request::parse("seek(1)\nseek(2)").unwrap_err().kind,
            ErrorKind::Parse
        );
        assert_eq!(
            Request::parse("state(1)").unwrap_err().kind,
            ErrorKind::BadArgument
        );
    }

    #[test]
    fn the_endpoint_file_round_trips_and_tokens_differ() {
        let endpoint = Endpoint {
            port: 4242,
            token: random_token(),
            pid: 7,
        };
        assert_eq!(endpoint.token.len(), 32);
        assert_ne!(endpoint.token, random_token());
        let dir = std::env::temp_dir().join(format!("motioner_control_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("control.txt");
        std::fs::write(&file, endpoint.to_text()).unwrap();
        assert_eq!(Endpoint::read(&file).unwrap(), endpoint);
        std::fs::write(&file, "port=x\n").unwrap();
        assert!(Endpoint::read(&file).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_server_answers_through_the_ui_channel_and_checks_the_token() {
        let dir = std::env::temp_dir().join(format!("motioner_server_{}", std::process::id()));
        let file = dir.join("control.txt");
        let server = ControlServer::start(&file).unwrap();
        let endpoint = Endpoint::read(&file).unwrap();
        assert_eq!(&endpoint, server.endpoint());

        let client = thread::spawn(move || send(&endpoint, "seek(12)").unwrap());
        // Play the UI thread: poll until the request arrives, then answer.
        let pending = loop {
            if let Some(pending) = server.poll().pop() {
                break pending;
            }
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(pending.request, Ok(Request::Command(Command::Seek(12))));
        pending.reply_ok_summary("frame 12");
        assert_eq!(
            client.join().unwrap(),
            r#"{"ok":true,"request":"seek(12)","result":"frame 12"}"#
        );

        let wrong = Endpoint {
            token: "nope".to_owned(),
            ..server.endpoint().clone()
        };
        assert!(send(&wrong, "state()").unwrap().contains("unauthorized"));
        drop(server);
        assert!(!file.exists(), "the endpoint file is removed on drop");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
