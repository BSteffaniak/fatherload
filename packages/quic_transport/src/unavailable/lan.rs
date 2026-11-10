//! Browser builds cannot discover LAN hosts or serve native TCP descriptors.
use crate::HostDescriptor;
use std::{io, net::SocketAddr, time::Duration};
pub const DEFAULT_GAME_PORT: u16 = 4242;
#[must_use]
pub fn local_machine_name() -> String {
    "Browser player".to_owned()
}
fn unavailable() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "Online play is not available in this browser build",
    )
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LanGameAdvertisement {
    pub instance_name: String,
    pub host_name: String,
    pub game_addr: SocketAddr,
    pub descriptor_addr: SocketAddr,
    pub session_id: String,
}

impl LanGameAdvertisement {
    #[must_use]
    pub fn from_descriptor(descriptor: &HostDescriptor) -> Self {
        let machine_name = local_machine_name();
        let game_addr = descriptor.host_addr;
        let descriptor_addr = SocketAddr::new(game_addr.ip(), 0);
        Self {
            instance_name: format!("Fatherload on {machine_name}"),
            host_name: format!("{}.local.", "browser"),
            game_addr,
            descriptor_addr,
            session_id: format!("fatherload-{}", game_addr.port()),
        }
    }

    #[must_use]
    pub const fn with_descriptor_addr(mut self, descriptor_addr: SocketAddr) -> Self {
        self.descriptor_addr = descriptor_addr;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LanDiscoveredGame {
    pub instance_name: String,
    pub host_name: String,
    pub game_addr: SocketAddr,
    pub descriptor_addr: SocketAddr,
    pub session_id: String,
}

pub struct LanDiscoveryPublisher;
impl LanDiscoveryPublisher {
    /// # Errors
    /// Always returns an error: native networking is unavailable on this target.
    pub fn publish(_: &LanGameAdvertisement) -> Result<Self, io::Error> {
        Err(unavailable())
    }
}
pub struct LanDescriptorServer;
impl LanDescriptorServer {
    /// # Errors
    /// Always returns an error: native networking is unavailable on this target.
    pub fn serve(_: SocketAddr, _: &HostDescriptor) -> io::Result<Self> {
        Err(unavailable())
    }
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        unreachable!("browser descriptor server cannot be constructed")
    }
}
pub struct LanDiscoveryBrowser;
impl LanDiscoveryBrowser {
    /// # Errors
    /// Always returns an error: native networking is unavailable on this target.
    pub fn new() -> io::Result<Self> {
        Err(unavailable())
    }
    /// # Errors
    /// Always returns an error: native networking is unavailable on this target.
    pub fn browse_for(&self, _: Duration) -> io::Result<Vec<LanDiscoveredGame>> {
        Err(unavailable())
    }
}
/// # Errors
/// Always returns an error: native networking is unavailable on this target.
pub fn fetch_descriptor(_: SocketAddr) -> io::Result<HostDescriptor> {
    Err(unavailable())
}
#[must_use]
pub const fn patch_descriptor_addr_for_lan(
    descriptor: HostDescriptor,
    addr: SocketAddr,
) -> HostDescriptor {
    descriptor.with_host_addr(addr)
}

#[must_use]
pub const fn likely_lan_ip() -> Option<std::net::IpAddr> {
    None
}
