//! LAN discovery off the game thread.
//!
//! mDNS browsing and the descriptor fetch both block, so each runs on a short-lived helper
//! thread and reports back over a channel the game loop polls without blocking.

use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    time::Duration,
};

use fatherload_quic_transport::{
    HostDescriptor,
    lan::{
        LanDiscoveredGame, LanDiscoveryBrowser, fetch_descriptor, patch_descriptor_addr_for_lan,
    },
};

/// How long one LAN browse listens for advertisements.
pub const LAN_BROWSE_DURATION: Duration = Duration::from_millis(1_500);

/// A background job that yields one result.
#[derive(Debug)]
pub struct Background<T> {
    rx: Receiver<Result<T, String>>,
}

impl<T: Send + 'static> Background<T> {
    fn spawn(name: &str, job: impl FnOnce() -> Result<T, String> + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                let _ignored = tx.send(job());
            });
        if let Err(error) = spawned {
            let (tx, failed) = mpsc::channel();
            let _ignored = tx.send(Err(format!("could not start {name}: {error}")));
            return Self { rx: failed };
        }
        Self { rx }
    }

    /// The result, once the job finished. Never blocks.
    #[must_use]
    pub fn poll(&self) -> Option<Result<T, String>> {
        match self.rx.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("background job stopped".to_owned())),
        }
    }
}

/// Browses the LAN for advertised sessions for [`LAN_BROWSE_DURATION`].
#[must_use]
pub fn browse() -> Background<Vec<LanDiscoveredGame>> {
    Background::spawn("fatherload-lan-browse", || {
        LanDiscoveryBrowser::new()
            .and_then(|browser| browser.browse_for(LAN_BROWSE_DURATION))
            .map_err(|error| error.to_string())
    })
}

/// Fetches a discovered session's descriptor (pinned certificate) and patches its address.
#[must_use]
pub fn fetch(game: &LanDiscoveredGame) -> Background<HostDescriptor> {
    let descriptor_addr = game.descriptor_addr;
    let game_addr = game.game_addr;
    Background::spawn("fatherload-lan-descriptor-fetch", move || {
        fetch_descriptor(descriptor_addr)
            .map(|descriptor| patch_descriptor_addr_for_lan(descriptor, game_addr))
            .map_err(|error| error.to_string())
    })
}
