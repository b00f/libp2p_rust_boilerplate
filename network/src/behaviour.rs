use crate::config::Config;
use crate::message::Message;
use crate::swarm_api::{SwarmApi, SwarmEvent};
use cid::Cid;
use libp2p::core::identity::Keypair;
use libp2p::gossipsub::protocol::MessageId;
use libp2p::gossipsub::{
    error::PublishError, Gossipsub, GossipsubConfigBuilder, GossipsubEvent, GossipsubMessage,
    MessageAuthenticity, Topic, TopicHash,
};
use libp2p::identify::{Identify, IdentifyEvent};
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{Kademlia, KademliaConfig, KademliaEvent, QueryId};
use libp2p::mdns::{Mdns, MdnsEvent};
use libp2p::multiaddr::Protocol;
use libp2p::ping::{
    handler::{PingFailure, PingSuccess},
    Ping, PingEvent,
};
use libp2p::NetworkBehaviour;
use libp2p::{
    core::PeerId,
    swarm::{toggle::Toggle, NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters},
};
use libp2p_bitswap::{Bitswap, BitswapEvent};
use log::{debug, trace, warn};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use std::{task::Context, task::Poll};

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "BehaviourEventOut", poll_method = "poll")]
pub struct Behaviour {
    swarm_api: SwarmApi,
    gossipsub: Gossipsub,
    mdns: Toggle<Mdns>,
    ping: Ping,
    identify: Identify,
    kademlia: Toggle<Kademlia<MemoryStore>>,
    bitswap: Bitswap,
    #[behaviour(ignore)]
    peers: HashSet<PeerId>,
    #[behaviour(ignore)]
    events: Vec<BehaviourEventOut>,
}

#[derive(Debug)]
pub enum BehaviourEventOut {
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    BitswapReceivedBlock(PeerId, Cid, Box<[u8]>),
    BitswapReceivedWant(PeerId, Cid),
    GossipMessage {
        source: Option<PeerId>,
        topics: Vec<TopicHash>,
        message: Message,
    },
}

impl Behaviour {
    fn poll<TBehaviourIn>(
        &mut self,
        _: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<TBehaviourIn, BehaviourEventOut>> {
        if !self.events.is_empty() {
            return Poll::Ready(NetworkBehaviourAction::GenerateEvent(self.events.remove(0)));
        }
        Poll::Pending
    }

    pub fn new(local_key: &Keypair, config: &Config) -> Self {
        // To content-address message, we can take the hash of message and use it as an ID.
        let message_id_fn = |message: &GossipsubMessage| {
            let mut s = DefaultHasher::new();
            message.data.hash(&mut s);
            MessageId::from(s.finish().to_string())
        };

        let local_peer_id = local_key.public().into_peer_id();
        let gossipsub_config = GossipsubConfigBuilder::new()
            .heartbeat_interval(Duration::from_secs(10))
            .message_id_fn(message_id_fn) // content-address messages. No two messages of the same content will be propagated.
            .build();

        let mut bitswap = Bitswap::new();
        // Kademlia config
        let store = MemoryStore::new(local_peer_id.to_owned());
        let mut kad_config = KademliaConfig::default();
        let network = format!("/mangostine/kad/{}/kad/1.0.0", config.network_name);
        kad_config.set_protocol_name(network.as_bytes().to_vec());
        let kademlia_opt = if config.kademlia {
            let mut kademlia = Kademlia::with_config(local_peer_id.to_owned(), store, kad_config);
            for multiaddr in config.bootstrap_peers.iter() {
                let mut addr = multiaddr.to_owned();
                if let Some(Protocol::P2p(mh)) = addr.pop() {
                    let peer_id = PeerId::from_multihash(mh).unwrap();
                    kademlia.add_address(&peer_id, addr);
                    bitswap.connect(peer_id);
                } else {
                    warn!("Could not add addr {} to Kademlia DHT", multiaddr)
                }
            }
            if let Err(e) = kademlia.bootstrap() {
                warn!("Kademlia bootstrap failed: {}", e);
            }
            Some(kademlia)
        } else {
            None
        };

        let mdns_opt = if config.mdns {
            Some(Mdns::new().expect("Could not start mDNS"))
        } else {
            None
        };

        let identity = Identify::new(
            "ipfs/0.1.0".into(),
            format!("mangostine-{}", "0.1.0"),
            local_key.public(),
        );

        let swarm_api = SwarmApi::new();
        Behaviour {
            swarm_api: swarm_api,
            gossipsub: Gossipsub::new(
                MessageAuthenticity::Signed(local_key.clone()),
                gossipsub_config,
            ),
            mdns: mdns_opt.into(),
            ping: Ping::default(),
            identify: identity,
            kademlia: kademlia_opt.into(),
            bitswap: bitswap,
            events: vec![],
            peers: Default::default(),
        }
    }

    /// Bootstrap Kademlia network
    pub fn bootstrap(&mut self) -> Result<QueryId, String> {
        if let Some(active_kad) = self.kademlia.as_mut() {
            active_kad.bootstrap().map_err(|e| e.to_string())
        } else {
            Err("Kademlia is not activated".to_string())
        }
    }

    /// Publish data over the gossip network.
    pub fn publish(&mut self, topic: &Topic, data: impl Into<Vec<u8>>) -> Result<(), PublishError> {
        self.gossipsub.publish(topic, data)
    }

    /// Subscribe to a gossip topic.
    pub fn subscribe(&mut self, topic: Topic) -> bool {
        self.gossipsub.subscribe(topic)
    }

    /// Adds peer to the peer set.
    pub fn add_peer(&mut self, peer_id: PeerId) {
        self.peers.insert(peer_id.clone());
        self.bitswap.connect(peer_id);
    }

    /// Adds peer to the peer set.
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        self.peers.remove(peer_id);
    }

    /// Adds peer to the peer set.
    pub fn peers(&self) -> &HashSet<PeerId> {
        &self.peers
    }
}

impl NetworkBehaviourEventProcess<IdentifyEvent> for Behaviour {
    fn inject_event(&mut self, event: IdentifyEvent) {
        match event {
            IdentifyEvent::Received {
                peer_id,
                info,
                observed_addr,
            } => {
                trace!("Identified Peer {}", peer_id);
                trace!("protocol_version {}", info.protocol_version);
                trace!("agent_version {}", info.agent_version);
                trace!("listening_ addresses {:?}", info.listen_addrs);
                trace!("observed_address {}", observed_addr);
                trace!("protocols {:?}", info.protocols);
            }
            IdentifyEvent::Sent { .. } => (),
            IdentifyEvent::Error { .. } => (),
        }
    }
}

impl NetworkBehaviourEventProcess<BitswapEvent> for Behaviour {
    fn inject_event(&mut self, event: BitswapEvent) {
        match event {
            BitswapEvent::ReceivedBlock(peer_id, cid, data) => {
                let cid = cid.to_bytes();
                match Cid::try_from(cid.as_slice()) {
                    Ok(cid) => self
                        .events
                        .push(BehaviourEventOut::BitswapReceivedBlock(peer_id, cid, data)),
                    Err(e) => warn!("Fail to convert Cid: {}", e.to_string()),
                }
            }
            BitswapEvent::ReceivedWant(peer_id, cid, _priority) => {
                let cid = cid.to_bytes();
                match Cid::try_from(cid.as_slice()) {
                    Ok(cid) => self
                        .events
                        .push(BehaviourEventOut::BitswapReceivedWant(peer_id, cid)),
                    Err(e) => warn!("Fail to convert Cid: {}", e.to_string()),
                }
            }
            BitswapEvent::ReceivedCancel(_peer_id, _cid) => {
                // TODO: Determine how to handle cancel
                trace!("BitswapEvent::ReceivedCancel, unimplemented");
            }
        }
    }
}

impl NetworkBehaviourEventProcess<MdnsEvent> for Behaviour {
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(list) => {
                for (peer, addr) in list {
                    trace!("mdns: Discovered peer {}", peer.to_base58());
                    self.add_peer(peer.clone());
                    self.kademlia.as_mut().unwrap().add_address(&peer, addr);
                }
            }
            MdnsEvent::Expired(list) => {
                if self.mdns.is_enabled() {
                    for (peer, _) in list {
                        if !self.mdns.as_ref().unwrap().has_node(&peer) {
                            self.remove_peer(&peer);
                        }
                    }
                }
            }
        }
    }
}

impl NetworkBehaviourEventProcess<KademliaEvent> for Behaviour {
    fn inject_event(&mut self, event: KademliaEvent) {
        match event {
            KademliaEvent::RoutingUpdated { peer, .. } => {
                self.add_peer(peer);
            }
            event => {
                trace!("kad: {:?}", event);
            }
        }
    }
}

impl NetworkBehaviourEventProcess<GossipsubEvent> for Behaviour {
    fn inject_event(&mut self, message: GossipsubEvent) {
        if let GossipsubEvent::Message(_, _, message) = message {
            self.events.push(BehaviourEventOut::GossipMessage {
                source: message.source,
                topics: message.topics,
                message: message.data.into(),
            })
        }
    }
}

impl NetworkBehaviourEventProcess<PingEvent> for Behaviour {
    fn inject_event(&mut self, event: PingEvent) {
        match event.result {
            Result::Ok(PingSuccess::Ping { rtt }) => {
                trace!(
                    "PingSuccess::Ping rtt to {} is {} ms",
                    event.peer.to_base58(),
                    rtt.as_millis()
                );
            }
            Result::Ok(PingSuccess::Pong) => {
                trace!("PingSuccess::Pong from {}", event.peer.to_base58());
            }
            Result::Err(PingFailure::Timeout) => {
                debug!("PingFailure::Timeout {}", event.peer.to_base58());
            }
            Result::Err(PingFailure::Other { error }) => {
                debug!("PingFailure::Other {}: {}", event.peer.to_base58(), error);
            }
        }
    }
}

impl NetworkBehaviourEventProcess<<SwarmApi as libp2p::swarm::NetworkBehaviour>::OutEvent> for Behaviour {
    fn inject_event(&mut self, event: <SwarmApi as libp2p::swarm::NetworkBehaviour>::OutEvent) {
        match event {
            SwarmEvent::PeerConnected(peer_id) => {
                self.events.push(BehaviourEventOut::PeerConnected(peer_id))
            }
            SwarmEvent::PeerDisconnected(peer_id) => {
                self.events.push(BehaviourEventOut::PeerDisconnected(peer_id))
            }
        }
    }
}