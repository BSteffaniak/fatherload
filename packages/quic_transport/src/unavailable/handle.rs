//! Native sockets are unavailable in a browser; offline sessions do not start a transport.
use crate::{Channel, ConnId, HostConfig, JoinTarget, NetCommand, NetConfig, NetEvent};
use std::{io, net::SocketAddr};
#[derive(Debug)]
pub struct NetHandle;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetThreadStopped;
impl std::fmt::Display for NetThreadStopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Online play is not available in this browser build")
    }
}
impl std::error::Error for NetThreadStopped {}
impl NetHandle {
    #[must_use]
    pub const fn shutdown(self) -> Vec<NetEvent> {
        Vec::new()
    }
    /// # Errors
    /// Always returns an error: native networking is unavailable on this target.
    pub fn start(_: NetConfig) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Online play is not available in this browser build",
        ))
    }
    /// # Errors
    /// Always returns an error: native networking is unavailable on this target.
    pub fn send(&self, _: NetCommand) -> Result<(), NetThreadStopped> {
        Err(NetThreadStopped)
    }
    /// Bind a host endpoint; expect [`NetEvent::HostReady`] or a host error.
    ///
    /// # Errors
    ///
    /// Returns [`NetThreadStopped`] if the thread has exited.
    /// # Errors
    /// Always returns an error: native networking is unavailable on this target.
    pub fn host(&self, bind: SocketAddr, config: HostConfig) -> Result<(), NetThreadStopped> {
        self.send(NetCommand::Host { bind, config })
    }

    /// Connect to a host; expect [`NetEvent::Connected`] or a join error.
    ///
    /// # Errors
    ///
    /// Returns [`NetThreadStopped`] if the thread has exited.
    /// # Errors
    /// Always returns an error: native networking is unavailable on this target.
    pub fn join(&self, target: JoinTarget) -> Result<(), NetThreadStopped> {
        self.send(NetCommand::Join { target })
    }

    /// Send a reliable, ordered message.
    ///
    /// # Errors
    ///
    /// Returns [`NetThreadStopped`] if the thread has exited.
    /// # Errors
    /// Always returns an error: native networking is unavailable on this target.
    pub fn send_reliable(&self, conn: ConnId, bytes: Vec<u8>) -> Result<(), NetThreadStopped> {
        self.send(NetCommand::Send {
            conn,
            channel: Channel::Reliable,
            bytes,
        })
    }

    /// Send a best-effort message (datagram, or control-stream fallback).
    ///
    /// # Errors
    ///
    /// Returns [`NetThreadStopped`] if the thread has exited.
    /// # Errors
    /// Always returns an error: native networking is unavailable on this target.
    pub fn send_unreliable(&self, conn: ConnId, bytes: Vec<u8>) -> Result<(), NetThreadStopped> {
        self.send(NetCommand::Send {
            conn,
            channel: Channel::Unreliable,
            bytes,
        })
    }

    /// Cleanly close one connection.
    ///
    /// # Errors
    ///
    /// Returns [`NetThreadStopped`] if the thread has exited.
    pub fn disconnect(
        &self,
        conn: ConnId,
        reason: impl Into<String>,
    ) -> Result<(), NetThreadStopped> {
        self.send(NetCommand::Disconnect {
            conn,
            reason: reason.into(),
        })
    }

    #[must_use]
    pub const fn drain(&self) -> Vec<NetEvent> {
        Vec::new()
    }
}
