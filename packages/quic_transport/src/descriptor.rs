//! Serializable connection descriptors, independent of the native TLS implementation.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Everything a client needs to join a host with a pinned certificate. Serializable (JSON)
/// so it can be copied between processes or served over LAN.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HostDescriptor {
    /// Address to connect to.
    pub host_addr: SocketAddr,
    /// TLS server name in the certificate.
    pub server_name: String,
    /// DER-encoded self-signed host certificate.
    pub certificate_der: Vec<u8>,
}

impl HostDescriptor {
    /// Serialize as JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Parse from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON does not describe a descriptor.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Return a copy with a different connect address (e.g. a LAN IP instead of `0.0.0.0`).
    #[must_use]
    pub const fn with_host_addr(mut self, host_addr: SocketAddr) -> Self {
        self.host_addr = host_addr;
        self
    }
}
