use libp2p::Multiaddr;
use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub network_name: String,
    pub listening_multiaddr: Multiaddr,
    pub bootstrap_peers: Vec<Multiaddr>,
    pub mdns: bool,
    pub kademlia: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            network_name: "libp2p_test".to_string(),
            listening_multiaddr: "/ip4/0.0.0.0/tcp/0".parse().unwrap(),
            bootstrap_peers: vec![],
            mdns: true,
            kademlia: true,
        }
    }
}
