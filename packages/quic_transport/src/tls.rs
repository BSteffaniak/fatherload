//! Self-signed certificates, host descriptors and the rustls/Quinn configuration.
//!
//! The host generates a fresh self-signed certificate with `rcgen` every time it binds.
//! Clients either pin that exact certificate (descriptor join, ported from the legacy
//! `QuinnHostConnectionDescriptor` flow) or skip authentication entirely (direct-IP join).

use std::{net::SocketAddr, sync::Arc};

use quinn::{
    crypto::rustls::{QuicClientConfig, QuicServerConfig},
    rustls::{
        self, DigitallySignedStruct, SignatureScheme,
        client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        crypto::CryptoProvider,
        pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime},
    },
};
use serde::{Deserialize, Serialize};

use crate::config::NetConfig;

/// Default TLS server name used for self-signed host certificates.
pub const DEFAULT_SERVER_NAME: &str = "localhost";

/// ALPN protocol identifier negotiated by both sides.
pub const ALPN: &[u8] = b"drillgame-quic/1";

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

pub struct HostIdentity {
    pub certificate: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
}

pub fn generate_identity(server_name: &str) -> Result<HostIdentity, String> {
    let certified_key = rcgen::generate_simple_self_signed(vec![server_name.to_owned()])
        .map_err(|error| format!("certificate generation failed: {error}"))?;
    let certificate = certified_key.cert.der().clone();
    let key = PrivateKeyDer::Pkcs8(certified_key.key_pair.serialize_der().into());
    Ok(HostIdentity { certificate, key })
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

pub fn transport_config(config: &NetConfig) -> Result<quinn::TransportConfig, String> {
    let mut transport = quinn::TransportConfig::default();
    transport.keep_alive_interval(Some(config.keep_alive_interval));
    transport.max_idle_timeout(Some(
        quinn::IdleTimeout::try_from(config.idle_timeout)
            .map_err(|error| format!("invalid idle timeout: {error}"))?,
    ));
    Ok(transport)
}

pub fn server_config(
    identity: HostIdentity,
    config: &NetConfig,
) -> Result<quinn::ServerConfig, String> {
    let mut tls = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|error| error.to_string())?
        .with_no_client_auth()
        .with_single_cert(vec![identity.certificate], identity.key)
        .map_err(|error| error.to_string())?;
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let crypto = QuicServerConfig::try_from(tls).map_err(|error| error.to_string())?;
    let mut server = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    server.transport_config(Arc::new(transport_config(config)?));
    Ok(server)
}

pub fn client_config(
    pinned: Option<CertificateDer<'static>>,
    config: &NetConfig,
) -> Result<quinn::ClientConfig, String> {
    let provider = provider();
    let verifier = Arc::new(PinnedCertVerifier {
        pinned,
        provider: Arc::clone(&provider),
    });
    let mut tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|error| error.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let crypto = QuicClientConfig::try_from(tls).map_err(|error| error.to_string())?;
    let mut client = quinn::ClientConfig::new(Arc::new(crypto));
    client.transport_config(Arc::new(transport_config(config)?));
    Ok(client)
}

/// Accepts exactly the pinned certificate, or (when `pinned` is `None`) any certificate.
/// Handshake signatures are always verified, so the peer must hold the certificate's key.
#[derive(Debug)]
struct PinnedCertVerifier {
    pinned: Option<CertificateDer<'static>>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for PinnedCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match &self.pinned {
            Some(pinned) if pinned.as_ref() != end_entity.as_ref() => Err(
                rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer),
            ),
            _ => Ok(ServerCertVerified::assertion()),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_json_round_trips() {
        let descriptor = HostDescriptor {
            host_addr: "127.0.0.1:4243".parse().expect("address parses"),
            server_name: DEFAULT_SERVER_NAME.to_owned(),
            certificate_der: vec![1, 2, 3],
        };
        let json = descriptor.to_json().expect("serializes");
        assert_eq!(
            HostDescriptor::from_json(&json).expect("parses"),
            descriptor
        );
    }

    #[test]
    fn generates_identity_and_configs() {
        let identity = generate_identity(DEFAULT_SERVER_NAME).expect("identity");
        let certificate = identity.certificate.clone();
        let config = NetConfig::default();
        server_config(identity, &config).expect("server config");
        client_config(Some(certificate), &config).expect("pinned client config");
        client_config(None, &config).expect("unauthenticated client config");
    }
}
