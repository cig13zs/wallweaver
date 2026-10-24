use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};

use interprocess::TryClone;
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{Listener, ListenerOptions, Stream};

use crate::frame::{FrameError, read_frame, read_frame_bytes, write_frame};
use crate::{
    ClientMessage, ErrorCode, Event, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION, RemoteError, Request,
    Response, ServerMessage, negotiate,
};

/// Errors from the transport layer.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    /// Framing or socket error.
    #[error(transparent)]
    Frame(#[from] FrameError),
    /// Could not create, bind or connect the socket.
    #[error("{context}: {source}")]
    Io {
        /// What was being done.
        context: String,
        /// The OS error.
        source: io::Error,
    },
    /// The daemon refused the handshake.
    #[error("the daemon refused the connection: {0}")]
    Rejected(String),
    /// The daemon answered with an error.
    #[error(transparent)]
    Remote(#[from] RemoteError),
    /// The peer broke the protocol (wrong message at the wrong time).
    #[error("protocol error: {0}")]
    Protocol(String),
    /// The connecting process belongs to another user.
    #[error("refused a connection from another user")]
    PeerNotAllowed,
    /// The socket folder exists but is not private to this user.
    #[error("{0} is not private to this user; refusing to use it")]
    UnsafeSocketDir(String),
}

impl IpcError {
    fn io(context: impl Into<String>, source: io::Error) -> Self {
        IpcError::Io {
            context: context.into(),
            source,
        }
    }
}

/// Where the daemon listens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// A Unix domain socket at this path.
    Path(PathBuf),
    /// A named pipe (Windows) with this name.
    Pipe(String),
}

impl Endpoint {
    /// The per-user endpoint the daemon uses by default.
    ///
    /// On Unix the socket lives in `$XDG_RUNTIME_DIR/wallweaver/` (or a
    /// `wallweaver-<uid>` folder in the temp directory when that isn't set),
    /// a folder created with mode 0700 and checked to belong to this user.
    /// On Windows it is the named pipe `wallweaver-<user name>`, created with
    /// a security descriptor that only admits its owner and SYSTEM.
    pub fn default_for_user() -> Endpoint {
        #[cfg(unix)]
        {
            let dir = match std::env::var_os("XDG_RUNTIME_DIR") {
                Some(d) if !d.is_empty() => PathBuf::from(d).join("wallweaver"),
                _ => std::env::temp_dir().join(format!("wallweaver-{}", current_uid())),
            };
            Endpoint::Path(dir.join("daemon.sock"))
        }
        #[cfg(windows)]
        {
            let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
            let safe: String = user
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            Endpoint::Pipe(format!("wallweaver-{safe}"))
        }
        #[cfg(not(any(unix, windows)))]
        {
            Endpoint::Path(std::env::temp_dir().join("wallweaver/daemon.sock"))
        }
    }

    fn name(&self) -> io::Result<interprocess::local_socket::Name<'_>> {
        match self {
            Endpoint::Path(p) => p
                .as_path()
                .to_fs_name::<interprocess::local_socket::GenericFilePath>(),
            Endpoint::Pipe(n) => n
                .as_str()
                .to_ns_name::<interprocess::local_socket::GenericNamespaced>(),
        }
    }
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Endpoint::Path(p) => write!(f, "{}", p.display()),
            Endpoint::Pipe(n) => write!(f, "\\\\.\\pipe\\{n}"),
        }
    }
}

#[cfg(unix)]
fn current_uid() -> u32 {
    // SAFETY: geteuid has no preconditions, cannot fail and touches no memory.
    unsafe { libc::geteuid() }
}

/// Create (or check) the private folder that holds the socket.
#[cfg(unix)]
fn prepare_socket_dir(dir: &Path) -> Result<(), IpcError> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    match std::fs::DirBuilder::new()
        .mode(0o700)
        .recursive(false)
        .create(dir)
    {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = dir.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| IpcError::io(format!("creating {}", parent.display()), e))?;
            }
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(dir)
                .map_err(|e| IpcError::io(format!("creating {}", dir.display()), e))?;
        }
        Err(e) => return Err(IpcError::io(format!("creating {}", dir.display()), e)),
    }
    let meta = std::fs::symlink_metadata(dir)
        .map_err(|e| IpcError::io(format!("checking {}", dir.display()), e))?;
    let private =
        meta.is_dir() && meta.uid() == current_uid() && meta.permissions().mode() & 0o077 == 0;
    if !private {
        return Err(IpcError::UnsafeSocketDir(dir.display().to_string()));
    }
    Ok(())
}

/// The daemon side: a bound listener.
#[derive(Debug)]
pub struct Server {
    listener: Listener,
    endpoint: Endpoint,
}

impl Server {
    /// Bind the endpoint. A stale socket file left by a crashed daemon is
    /// replaced; a live daemon on the same endpoint makes this fail.
    pub fn bind(endpoint: &Endpoint) -> Result<Server, IpcError> {
        #[cfg(unix)]
        if let Endpoint::Path(p) = endpoint {
            if let Some(dir) = p.parent() {
                prepare_socket_dir(dir)?;
            }
            // Only remove the old socket if nobody answers on it.
            if p.exists()
                && Stream::connect(
                    endpoint
                        .name()
                        .map_err(|e| IpcError::io("socket name", e))?,
                )
                .is_err()
            {
                let _ = std::fs::remove_file(p);
            }
        }
        let name = endpoint
            .name()
            .map_err(|e| IpcError::io(format!("socket name {endpoint}"), e))?;
        let opts = ListenerOptions::new().name(name).reclaim_name(true);
        #[cfg(unix)]
        let opts = {
            use interprocess::os::unix::local_socket::ListenerOptionsExt;
            opts.mode(0o600)
        };
        #[cfg(windows)]
        let opts = {
            use interprocess::os::windows::local_socket::ListenerOptionsExt;
            use interprocess::os::windows::security_descriptor::SecurityDescriptor;
            // Owner and SYSTEM only. No access for other users.
            let sddl = widestring::U16CString::from_str("D:P(A;;GA;;;OW)(A;;GA;;;SY)")
                .map_err(|_| IpcError::Protocol("bad security descriptor string".into()))?;
            let sd = SecurityDescriptor::deserialize(&sddl)
                .map_err(|e| IpcError::io("building pipe security descriptor", e))?;
            opts.security_descriptor(sd)
        };
        let listener = opts
            .create_sync()
            .map_err(|e| IpcError::io(format!("listening on {endpoint}"), e))?;
        Ok(Server {
            listener,
            endpoint: endpoint.clone(),
        })
    }

    /// The endpoint this server is bound to.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Wait for the next client. On Unix, connections from other users are
    /// refused here (the folder permissions should already prevent them).
    pub fn accept(&self) -> Result<Connection, IpcError> {
        let stream = self
            .listener
            .accept()
            .map_err(|e| IpcError::io("accepting a connection", e))?;
        #[cfg(unix)]
        {
            let creds = stream
                .peer_creds()
                .map_err(|e| IpcError::io("reading peer credentials", e))?;
            if creds.euid() != Some(current_uid()) {
                return Err(IpcError::PeerNotAllowed);
            }
        }
        Ok(Connection {
            stream,
            protocol: 0,
        })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Endpoint::Path(p) = &self.endpoint {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// What a connection received after the handshake.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    /// A request this build understands.
    Request {
        /// Echo this in the response.
        id: u64,
        /// The request.
        request: Request,
    },
    /// A well-formed request of a kind this build doesn't know. Answer it
    /// with [`ErrorCode::Unsupported`] and carry on.
    Unsupported {
        /// Echo this in the response.
        id: u64,
        /// What failed to decode.
        detail: String,
    },
}

/// The daemon's end of one client connection.
#[derive(Debug)]
pub struct Connection {
    stream: Stream,
    protocol: u32,
}

impl Connection {
    /// Run the server side of the handshake. Returns the agreed version.
    pub fn handshake(&mut self, daemon_version: &str) -> Result<u32, IpcError> {
        let hello: ClientMessage = read_frame(&mut self.stream)?;
        let ClientMessage::Hello {
            min_protocol,
            max_protocol,
            ..
        } = hello
        else {
            let _ = write_frame(
                &mut self.stream,
                &ServerMessage::Rejected {
                    reason: "expected hello".into(),
                },
            );
            return Err(IpcError::Protocol("first message was not hello".into()));
        };
        match negotiate(min_protocol, max_protocol) {
            Some(v) => {
                self.protocol = v;
                write_frame(
                    &mut self.stream,
                    &ServerMessage::Welcome {
                        protocol: v,
                        daemon_version: daemon_version.to_string(),
                    },
                )?;
                Ok(v)
            }
            None => {
                let reason = format!(
                    "this daemon speaks protocol {MIN_PROTOCOL_VERSION} to {PROTOCOL_VERSION}, the client speaks {min_protocol} to {max_protocol}; update the older one"
                );
                let _ = write_frame(
                    &mut self.stream,
                    &ServerMessage::Rejected {
                        reason: reason.clone(),
                    },
                );
                Err(IpcError::Rejected(reason))
            }
        }
    }

    /// The protocol version agreed in the handshake.
    pub fn protocol(&self) -> u32 {
        self.protocol
    }

    /// Wait for the next request.
    pub fn recv(&mut self) -> Result<Incoming, IpcError> {
        let bytes = read_frame_bytes(&mut self.stream)?;
        decode_incoming(&bytes)
    }

    /// Answer a request.
    pub fn respond(
        &mut self,
        id: u64,
        result: Result<Response, RemoteError>,
    ) -> Result<(), IpcError> {
        write_frame(&mut self.stream, &ServerMessage::Response { id, result })?;
        Ok(())
    }

    /// Push an event.
    pub fn send_event(&mut self, event: Event) -> Result<(), IpcError> {
        write_frame(&mut self.stream, &ServerMessage::Event { event })?;
        Ok(())
    }

    /// A second handle to the same connection, for writing events from
    /// another thread while this one reads requests.
    pub fn try_clone(&self) -> Result<Connection, IpcError> {
        Ok(Connection {
            stream: self
                .stream
                .try_clone()
                .map_err(|e| IpcError::io("cloning connection", e))?,
            protocol: self.protocol,
        })
    }
}

pub(crate) fn decode_incoming(bytes: &[u8]) -> Result<Incoming, IpcError> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(FrameError::Json)?;
    if value.get("kind").and_then(|k| k.as_str()) != Some("request") {
        return Err(IpcError::Protocol("expected a request".into()));
    }
    let id = value
        .get("id")
        .and_then(|i| i.as_u64())
        .ok_or_else(|| IpcError::Protocol("request without an id".into()))?;
    let raw = value
        .get("request")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    match serde_json::from_value::<Request>(raw) {
        Ok(request) => Ok(Incoming::Request { id, request }),
        Err(e) => Ok(Incoming::Unsupported {
            id,
            detail: e.to_string(),
        }),
    }
}

/// The UI side: a connected, handshaken client.
#[derive(Debug)]
pub struct Client {
    stream: Stream,
    next_id: u64,
    events: VecDeque<Event>,
    protocol: u32,
    daemon_version: String,
}

impl Client {
    /// Connect and handshake.
    pub fn connect(endpoint: &Endpoint, client_name: &str) -> Result<Client, IpcError> {
        let name = endpoint
            .name()
            .map_err(|e| IpcError::io(format!("socket name {endpoint}"), e))?;
        let mut stream = Stream::connect(name)
            .map_err(|e| IpcError::io(format!("connecting to the daemon at {endpoint}"), e))?;
        write_frame(
            &mut stream,
            &ClientMessage::Hello {
                min_protocol: MIN_PROTOCOL_VERSION,
                max_protocol: PROTOCOL_VERSION,
                client: client_name.to_string(),
            },
        )?;
        match read_frame::<_, ServerMessage>(&mut stream)? {
            ServerMessage::Welcome {
                protocol,
                daemon_version,
            } => Ok(Client {
                stream,
                next_id: 1,
                events: VecDeque::new(),
                protocol,
                daemon_version,
            }),
            ServerMessage::Rejected { reason } => Err(IpcError::Rejected(reason)),
            other => Err(IpcError::Protocol(format!(
                "unexpected handshake reply: {other:?}"
            ))),
        }
    }

    /// Protocol version agreed with the daemon.
    pub fn protocol(&self) -> u32 {
        self.protocol
    }

    /// The daemon's version string.
    pub fn daemon_version(&self) -> &str {
        &self.daemon_version
    }

    /// Send a request and wait for its response. Events that arrive in the
    /// meantime are queued for [`Client::next_event`].
    pub fn request(&mut self, request: Request) -> Result<Response, IpcError> {
        let id = self.next_id;
        self.next_id += 1;
        write_frame(&mut self.stream, &ClientMessage::Request { id, request })?;
        loop {
            match read_frame::<_, ServerMessage>(&mut self.stream)? {
                ServerMessage::Response { id: got, result } if got == id => {
                    return result.map_err(IpcError::Remote);
                }
                ServerMessage::Response { id: got, .. } => {
                    return Err(IpcError::Protocol(format!(
                        "response for request {got} while waiting for {id}"
                    )));
                }
                ServerMessage::Event { event } => self.events.push_back(event),
                other => {
                    return Err(IpcError::Protocol(format!("unexpected message: {other:?}")));
                }
            }
        }
    }

    /// Wait for the next event (after [`Request::Subscribe`]).
    pub fn next_event(&mut self) -> Result<Event, IpcError> {
        if let Some(e) = self.events.pop_front() {
            return Ok(e);
        }
        match read_frame::<_, ServerMessage>(&mut self.stream)? {
            ServerMessage::Event { event } => Ok(event),
            other => Err(IpcError::Protocol(format!(
                "unexpected message while waiting for events: {other:?}"
            ))),
        }
    }
}

/// Answer for requests a daemon build doesn't support.
pub fn unsupported(detail: &str) -> RemoteError {
    RemoteError::new(
        ErrorCode::Unsupported,
        format!(
            "the daemon does not support this request ({detail}); it may be older than the app"
        ),
    )
}

#[cfg(not(unix))]
#[allow(dead_code)]
fn prepare_socket_dir(_dir: &Path) -> Result<(), IpcError> {
    Ok(())
}
