use std::net::{Ipv4Addr, SocketAddrV4};

use serde::{Deserialize, Serialize, de::Visitor};

#[derive(Debug, Clone, Serialize)]
pub struct TrackerRequest {
    pub peer_id: String,
    pub port: u16,
    pub uploaded: usize,
    pub downloaded: usize,
    pub left: usize,
    pub compact: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrackerResponse {
    pub interval: usize,
    pub peers: Peers,
}

#[derive(Debug, Clone)]
pub struct Peers(pub Vec<SocketAddrV4>);
struct PeersVisitor;

impl<'de> Visitor<'de> for PeersVisitor {
    type Value = Peers;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("6 bytes, the first 4 bytes are peer's IP address and the last 2 are a peer's port number")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if v.len() % 6 != 0 {
            Err(E::custom("Invalid peer list length"))
        } else {
            Ok(Peers(
                v.chunks_exact(6)
                    .map(|chunk| {
                        SocketAddrV4::new(
                            Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]),
                            u16::from_be_bytes([chunk[4], chunk[5]]),
                        )
                    })
                    .collect(),
            ))
        }
    }
}

impl<'de> Deserialize<'de> for Peers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_bytes(PeersVisitor)
    }
}

pub fn url_encode(bytes: &[u8; 20]) -> String {
    let mut encoded = String::with_capacity(40);
    for &byte in bytes {
        encoded.push('%');
        encoded.push_str(&hex::encode([byte]));
    }
    encoded
}
