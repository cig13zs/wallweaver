//! IPC threads. One thread accepts connections; each connection gets a
//! reader thread. Requests go to the daemon's main thread over a channel,
//! which is woken through the desktop host's waker.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use ww_desktop::Waker;
use ww_ipc::{Connection, Event, Incoming, RemoteError, Request, Response, Server, unsupported};

/// Connection id.
pub type ConnId = u64;

/// Messages for the main thread.
pub enum Msg {
    /// A client request and where to send the answer.
    Request {
        request: Request,
        reply: Sender<Result<Response, RemoteError>>,
    },
    /// A client subscribed to events.
    Subscribe { conn: ConnId, sink: EventSink },
    /// A client went away.
    Disconnected(ConnId),
    /// A wallpaper finished loading on a loader thread.
    Loaded(crate::daemon::LoadResult),
}

/// Sends events to one client.
#[derive(Clone)]
pub struct EventSink(Arc<Mutex<Connection>>);

impl EventSink {
    /// Send an event; returns false if the client is gone.
    pub fn send(&self, event: &Event) -> bool {
        match self.0.lock() {
            Ok(mut c) => c.send_event(event.clone()).is_ok(),
            Err(_) => false,
        }
    }
}

/// Start accepting clients in the background.
pub fn spawn(server: Server, to_main: Sender<Msg>, waker: Waker, version: &'static str) {
    let next_id = Arc::new(AtomicU64::new(1));
    let _ = std::thread::Builder::new()
        .name("ww-ipc-accept".into())
        .spawn(move || {
            loop {
                let conn = match server.accept() {
                    Ok(c) => c,
                    Err(e) => {
                        crate::log(&format!("refused a connection: {e}"));
                        continue;
                    }
                };
                let id = next_id.fetch_add(1, Ordering::Relaxed);
                let tx = to_main.clone();
                let waker = waker.clone();
                let _ = std::thread::Builder::new()
                    .name("ww-ipc-conn".into())
                    .spawn(move || serve(id, conn, tx, waker, version));
            }
        });
}

fn serve(id: ConnId, mut conn: Connection, to_main: Sender<Msg>, waker: Waker, version: &str) {
    if let Err(e) = conn.handshake(version) {
        crate::log(&format!("handshake failed: {e}"));
        return;
    }
    let writer = match conn.try_clone() {
        Ok(w) => Arc::new(Mutex::new(w)),
        Err(_) => return,
    };
    loop {
        let incoming = match conn.recv() {
            Ok(i) => i,
            Err(_) => break,
        };
        let (rid, result) = match incoming {
            Incoming::Unsupported { id: rid, detail } => (rid, Err(unsupported(&detail))),
            Incoming::Request {
                id: rid,
                request: Request::Subscribe,
            } => {
                let _ = to_main.send(Msg::Subscribe {
                    conn: id,
                    sink: EventSink(writer.clone()),
                });
                waker.wake();
                (rid, Ok(Response::Ok))
            }
            Incoming::Request { id: rid, request } => {
                let (tx, rx) = channel();
                if to_main.send(Msg::Request { request, reply: tx }).is_err() {
                    break;
                }
                waker.wake();
                match rx.recv() {
                    Ok(r) => (rid, r),
                    Err(_) => break,
                }
            }
        };
        let ok = match writer.lock() {
            Ok(mut w) => w.respond(rid, result).is_ok(),
            Err(_) => false,
        };
        if !ok {
            break;
        }
    }
    let _ = to_main.send(Msg::Disconnected(id));
    waker.wake();
}
