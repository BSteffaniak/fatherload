//! [`NetHandle`]: the game loop's only interface to the network thread.

use std::{
    io,
    net::SocketAddr,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::Duration,
};

use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::{
    api::{Channel, ConnId, HostConfig, JoinTarget, NetCommand, NetEvent},
    config::NetConfig,
    network::Network,
};

/// Returned when a command is sent after the network thread has stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetThreadStopped;

impl std::fmt::Display for NetThreadStopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("network thread has stopped")
    }
}

impl std::error::Error for NetThreadStopped {}

/// Owns the network thread. Sending commands and draining events never block.
///
/// Dropping the handle performs [`NetHandle::shutdown`].
#[derive(Debug)]
pub struct NetHandle {
    commands: UnboundedSender<NetCommand>,
    events: mpsc::Receiver<NetEvent>,
    thread: Option<JoinHandle<()>>,
}

impl NetHandle {
    /// Start the network thread with its own multi-thread Tokio runtime.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime or the thread cannot be created.
    pub fn start(config: NetConfig) -> io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(config.worker_threads.max(1))
            .thread_name("fatherload-net-worker")
            .enable_all()
            .build()?;
        let (command_tx, command_rx) = unbounded_channel();
        let (event_tx, event_rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("fatherload-net".to_owned())
            .spawn(move || {
                runtime.block_on(Network::new(config, event_tx).run(command_rx));
                runtime.shutdown_timeout(Duration::from_secs(1));
            })?;
        Ok(Self {
            commands: command_tx,
            events: event_rx,
            thread: Some(thread),
        })
    }

    /// Queue a command for the network thread.
    ///
    /// # Errors
    ///
    /// Returns [`NetThreadStopped`] if the thread has exited.
    pub fn send(&self, command: NetCommand) -> Result<(), NetThreadStopped> {
        self.commands.send(command).map_err(|_| NetThreadStopped)
    }

    /// Bind a host endpoint; expect [`NetEvent::HostReady`] or a host error.
    ///
    /// # Errors
    ///
    /// Returns [`NetThreadStopped`] if the thread has exited.
    pub fn host(&self, bind: SocketAddr, config: HostConfig) -> Result<(), NetThreadStopped> {
        self.send(NetCommand::Host { bind, config })
    }

    /// Connect to a host; expect [`NetEvent::Connected`] or a join error.
    ///
    /// # Errors
    ///
    /// Returns [`NetThreadStopped`] if the thread has exited.
    pub fn join(&self, target: JoinTarget) -> Result<(), NetThreadStopped> {
        self.send(NetCommand::Join { target })
    }

    /// Send a reliable, ordered message.
    ///
    /// # Errors
    ///
    /// Returns [`NetThreadStopped`] if the thread has exited.
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

    /// Next pending event, if any. Never blocks.
    #[must_use]
    pub fn try_recv(&self) -> Option<NetEvent> {
        self.events.try_recv().ok()
    }

    /// All currently pending events. Never blocks.
    #[must_use]
    pub fn drain(&self) -> Vec<NetEvent> {
        self.events.try_iter().collect()
    }

    /// Wait up to `timeout` for the next event. Intended for headless tools and tests,
    /// not the game loop.
    #[must_use]
    pub fn recv_timeout(&self, timeout: Duration) -> Option<NetEvent> {
        self.events.recv_timeout(timeout).ok()
    }

    /// Disconnect every connection cleanly (bounded by `close_timeout`), stop the thread and
    /// return any events that were still pending, ending with [`NetEvent::ShutdownComplete`].
    #[must_use]
    pub fn shutdown(mut self) -> Vec<NetEvent> {
        self.stop();
        self.drain()
    }

    fn stop(&mut self) {
        let _ignored = self.commands.send(NetCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ignored = thread.join();
        }
    }
}

impl Drop for NetHandle {
    fn drop(&mut self) {
        self.stop();
    }
}
