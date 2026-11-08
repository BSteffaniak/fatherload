//! LAN discovery: mDNS publish/browse plus a tiny TCP endpoint that serves the host
//! descriptor (ported from the game crate's `lan_discovery` module).
//!
//! This API is synchronous and independent of [`crate::NetHandle`]: a host publishes a
//! [`LanGameAdvertisement`] and serves its [`HostDescriptor`] with [`LanDescriptorServer`];
//! a client browses with [`LanDiscoveryBrowser`] (call it off the game thread, it blocks for
//! the browse duration), fetches the descriptor with [`fetch_descriptor`], patches its
//! address with [`patch_descriptor_addr_for_lan`] and joins with
//! [`crate::JoinTarget::Descriptor`].

use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};

use crate::tls::HostDescriptor;

pub const SERVICE_TYPE: &str = "_drillgame._udp.local.";
pub const PROTOCOL_VERSION: &str = "1";
pub const DEFAULT_GAME_PORT: u16 = 4_242;
pub const RECOMMENDED_DESCRIPTOR_PORT: u16 = 4_243;
pub const MDNS_PORT: u16 = 5_353;
pub const RECOMMENDED_BROADCAST_PORT: u16 = 34_242;
const DESCRIPTOR_READ_LIMIT_BYTES: u64 = 128 * 1024;
const DEFAULT_DESCRIPTOR_FETCH_TIMEOUT: Duration = Duration::from_secs(3);

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
            instance_name: format!("Drillgame on {machine_name}"),
            host_name: format!("{}.local.", dns_label(&machine_name)),
            game_addr,
            descriptor_addr,
            session_id: format!("drillgame-{}", game_addr.port()),
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

pub struct LanDiscoveryPublisher {
    daemon: ServiceDaemon,
    fullname: String,
}

impl LanDiscoveryPublisher {
    /// Publish this LAN game through mDNS until the returned publisher is dropped.
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS daemon cannot be started or service info cannot be registered.
    pub fn publish(advertisement: &LanGameAdvertisement) -> Result<Self, mdns_sd::Error> {
        let daemon = ServiceDaemon::new()?;
        let game_addr = advertisement.game_addr.to_string();
        let descriptor_addr = advertisement.descriptor_addr.to_string();
        let properties: [(&str, &str); 4] = [
            ("protocol", PROTOCOL_VERSION),
            ("session_id", advertisement.session_id.as_str()),
            ("game_addr", game_addr.as_str()),
            ("descriptor_addr", descriptor_addr.as_str()),
        ];
        let service = ServiceInfo::new(
            SERVICE_TYPE,
            &advertisement.instance_name,
            &advertisement.host_name,
            advertisement.game_addr.ip().to_string(),
            advertisement.game_addr.port(),
            &properties[..],
        )?;
        let fullname = service.get_fullname().to_owned();
        daemon.register(service)?;
        Ok(Self { daemon, fullname })
    }
}

impl Drop for LanDiscoveryPublisher {
    fn drop(&mut self) {
        let _ignored = self.daemon.unregister(&self.fullname);
    }
}

#[derive(Debug)]
pub struct LanDescriptorServer {
    local_addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl LanDescriptorServer {
    /// Serve the descriptor JSON over a tiny local TCP endpoint until dropped.
    ///
    /// # Errors
    ///
    /// Returns an error if the TCP listener cannot bind or descriptor serialization fails.
    pub fn serve(
        bind_addr: SocketAddr,
        descriptor: &HostDescriptor,
    ) -> Result<Self, std::io::Error> {
        let json = serde_json::to_vec(descriptor).map_err(std::io::Error::other)?;
        let listener = TcpListener::bind(bind_addr)?;
        listener.set_nonblocking(true)?;
        let local_addr = listener.local_addr()?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let handle = thread::Builder::new()
            .name("drillgame-lan-descriptor".to_owned())
            .spawn(move || {
                while !thread_shutdown.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _peer)) => {
                            // Accepted sockets inherit O_NONBLOCK from the listener on BSD/macOS;
                            // a non-blocking write_all could fail mid-descriptor with WouldBlock.
                            let _ignored = stream.set_nonblocking(false);
                            let _ignored =
                                stream.set_write_timeout(Some(DEFAULT_DESCRIPTOR_FETCH_TIMEOUT));
                            let _ignored = stream.write_all(&json);
                            let _ignored = stream.flush();
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(25));
                        }
                        Err(_error) => break,
                    }
                }
            })?;
        Ok(Self {
            local_addr,
            shutdown,
            handle: Some(handle),
        })
    }

    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}

impl Drop for LanDescriptorServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ignored = TcpStream::connect_timeout(&self.local_addr, Duration::from_millis(50));
        if let Some(handle) = self.handle.take() {
            let _ignored = handle.join();
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LanBrowseReport {
    pub games: Vec<LanDiscoveredGame>,
    pub events_seen: usize,
    pub resolved_seen: usize,
    pub unusable_resolved_services: Vec<String>,
}

pub struct LanDiscoveryBrowser {
    daemon: ServiceDaemon,
}

impl LanDiscoveryBrowser {
    /// Create an mDNS browser for Drillgame LAN sessions.
    ///
    /// # Errors
    ///
    /// Returns an error if the mDNS daemon cannot be started.
    pub fn new() -> Result<Self, mdns_sd::Error> {
        Ok(Self {
            daemon: ServiceDaemon::new()?,
        })
    }

    /// Browse for LAN sessions for a bounded amount of time.
    ///
    /// # Errors
    ///
    /// Returns an error if mDNS browsing fails.
    pub fn browse_for(&self, duration: Duration) -> Result<Vec<LanDiscoveredGame>, mdns_sd::Error> {
        self.browse_report_for(duration).map(|report| report.games)
    }

    /// Browse for LAN sessions and retain diagnostic details about unusable mDNS results.
    ///
    /// # Errors
    ///
    /// Returns an error if mDNS browsing fails.
    pub fn browse_report_for(&self, duration: Duration) -> Result<LanBrowseReport, mdns_sd::Error> {
        let receiver = self.daemon.browse(SERVICE_TYPE)?;
        let deadline = std::time::Instant::now() + duration;
        let mut report = LanBrowseReport::default();
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if let Ok(event) = receiver.recv_timeout(remaining.min(Duration::from_millis(100))) {
                report.events_seen += 1;
                if let ServiceEvent::ServiceResolved(info) = event {
                    report.resolved_seen += 1;
                    if let Some(game) = discovered_game_from_service(&info) {
                        if !report.games.iter().any(|existing| existing == &game) {
                            report.games.push(game);
                        }
                    } else {
                        report
                            .unusable_resolved_services
                            .push(service_debug_summary(&info));
                    }
                }
            }
        }
        Ok(report)
    }
}

/// Fetch a descriptor from a LAN descriptor endpoint.
///
/// # Errors
///
/// Returns an error if the endpoint cannot be reached, read, or parsed.
pub fn fetch_descriptor(descriptor_addr: SocketAddr) -> Result<HostDescriptor, std::io::Error> {
    let stream = TcpStream::connect_timeout(&descriptor_addr, DEFAULT_DESCRIPTOR_FETCH_TIMEOUT)?;
    stream.set_read_timeout(Some(DEFAULT_DESCRIPTOR_FETCH_TIMEOUT))?;
    let mut json = Vec::new();
    stream
        .take(DESCRIPTOR_READ_LIMIT_BYTES)
        .read_to_end(&mut json)?;
    serde_json::from_slice(&json).map_err(std::io::Error::other)
}

#[must_use]
pub const fn patch_descriptor_addr_for_lan(
    mut descriptor: HostDescriptor,
    discovered_game_addr: SocketAddr,
) -> HostDescriptor {
    descriptor.host_addr = patch_non_lan_addr(descriptor.host_addr, discovered_game_addr.ip());
    descriptor
}

fn service_debug_summary(info: &ResolvedService) -> String {
    let properties = info
        .get_properties()
        .iter()
        .map(|property| format!("{}={}", property.key(), property.val_str()))
        .collect::<Vec<_>>()
        .join(" ");
    let addresses = info
        .get_addresses()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{} host={} port={} addresses=[{}] txt=[{}]",
        info.get_fullname(),
        info.get_hostname(),
        info.get_port(),
        addresses,
        properties
    )
}

fn resolved_service_addr(info: &ResolvedService) -> Option<SocketAddr> {
    info.get_addresses()
        .iter()
        .find(|address| address.is_ipv4())
        .map(mdns_sd::ScopedIp::to_ip_addr)
        .map(|address| SocketAddr::new(address, info.get_port()))
}

const fn patch_non_lan_addr(addr: SocketAddr, fallback_ip: IpAddr) -> SocketAddr {
    if addr.ip().is_loopback() || addr.ip().is_unspecified() {
        SocketAddr::new(fallback_ip, addr.port())
    } else {
        addr
    }
}

#[must_use]
pub fn likely_lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)).ok()?;
    socket
        .connect(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 80))
        .ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

fn discovered_game_from_service(info: &ResolvedService) -> Option<LanDiscoveredGame> {
    let properties = info.get_properties();
    let descriptor_addr = properties.get("descriptor_addr")?.val_str().parse().ok()?;
    let game_addr = properties
        .get("game_addr")
        .and_then(|property| property.val_str().parse().ok())
        .or_else(|| resolved_service_addr(info))?;
    let descriptor_addr = patch_non_lan_addr(descriptor_addr, game_addr.ip());
    let game_addr = patch_non_lan_addr(game_addr, descriptor_addr.ip());
    Some(LanDiscoveredGame {
        instance_name: info.get_fullname().split('.').next()?.to_owned(),
        host_name: info.get_hostname().to_owned(),
        game_addr,
        descriptor_addr,
        session_id: properties
            .get("session_id")
            .map_or_else(String::new, |property| property.val_str().to_owned()),
    })
}

#[must_use]
pub fn local_machine_name() -> String {
    hostname::get()
        .ok()
        .and_then(|name| name.into_string().ok())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Unknown Miner".to_owned())
}

fn dns_label(name: &str) -> String {
    let label: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    label.trim_matches('-').to_owned()
}

#[must_use]
pub const fn localhost_descriptor_bind_addr() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dns_label_sanitizes_hostname_for_mdns() {
        assert_eq!(dns_label("Braden's MacBook Pro"), "braden-s-macbook-pro");
    }

    #[test]
    fn advertisement_uses_descriptor_game_address() {
        let descriptor = HostDescriptor {
            host_addr: "192.168.1.20:4243".parse().expect("address parses"),
            server_name: "localhost".to_owned(),
            certificate_der: vec![1, 2, 3],
        };
        let advertisement = LanGameAdvertisement::from_descriptor(&descriptor)
            .with_descriptor_addr("192.168.1.20:4244".parse().expect("address parses"));

        assert_eq!(advertisement.game_addr, descriptor.host_addr);
        assert_eq!(advertisement.descriptor_addr.port(), 4244);
        assert!(advertisement.instance_name.starts_with("Drillgame on "));
    }

    #[test]
    fn descriptor_server_round_trips_descriptor() {
        let descriptor = HostDescriptor {
            host_addr: "127.0.0.1:4243".parse().expect("address parses"),
            server_name: "localhost".to_owned(),
            certificate_der: vec![1, 2, 3],
        };
        let server =
            LanDescriptorServer::serve("127.0.0.1:0".parse().expect("address parses"), &descriptor)
                .expect("server starts");

        let fetched = fetch_descriptor(server.local_addr()).expect("descriptor fetches");

        assert_eq!(fetched, descriptor);
    }
}
