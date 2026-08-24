//! Single-instance guard over the session bus. The first process to own
//! `dev.dictation.Tool` is the daemon; any later launch calls `Show` on it
//! (which raises the main window) and exits. This is what keeps a manual
//! launch and the systemd unit from both grabbing the keyboard and typing
//! every dictation twice.

use crate::state::EngineEvent;
use crossbeam_channel::Sender;
use zbus::blocking::Connection;
use zbus::fdo::{RequestNameFlags, RequestNameReply};

const NAME: &str = "dev.dictation.Tool";
const PATH: &str = "/dev/dictation/Tool";

struct Server {
    tx: Sender<EngineEvent>,
}

#[zbus::interface(name = "dev.dictation.Tool")]
impl Server {
    fn show(&self) {
        let _ = self.tx.send(EngineEvent::ShowWindow);
    }
}

pub enum Instance {
    /// We are the daemon. Holds the bus connection that owns the name (or
    /// `None` if there is no usable session bus -- then we simply run
    /// unguarded rather than refuse to start).
    Primary(#[allow(dead_code)] Option<Connection>),
    /// Another instance is running and has been asked to show its window.
    Secondary,
}

pub fn claim(tx: Sender<EngineEvent>) -> Instance {
    let conn = match Connection::session() {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("[instance] session bus unavailable ({err}); running without single-instance guard");
            return Instance::Primary(None);
        }
    };

    // Register the object before requesting the name so a `Show` arriving
    // immediately after acquisition cannot be lost.
    if let Err(err) = conn.object_server().at(PATH, Server { tx }) {
        eprintln!("[instance] could not export {PATH} ({err}); running without single-instance guard");
        return Instance::Primary(None);
    }

    // Explicit flags: zbus's *default* `RequestNameFlags` are
    // `AllowReplacement | ReplaceExisting | DoNotQueue`, which would let every
    // new launch silently steal the name from the running daemon.
    match conn.request_name_with_flags(NAME, RequestNameFlags::DoNotQueue.into()) {
        Ok(RequestNameReply::PrimaryOwner) | Ok(RequestNameReply::AlreadyOwner) => {
            Instance::Primary(Some(conn))
        }
        Ok(RequestNameReply::Exists) | Ok(RequestNameReply::InQueue) | Err(zbus::Error::NameTaken) => {
            println!("[instance] {NAME} is already owned; asking the running instance to show itself");
            let _ = conn.call_method(Some(NAME), PATH, Some(NAME), "Show", &());
            Instance::Secondary
        }
        Err(err) => {
            eprintln!("[instance] RequestName failed ({err}); running without single-instance guard");
            Instance::Primary(None)
        }
    }
}
