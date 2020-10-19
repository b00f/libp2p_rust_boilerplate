use crate::behaviour;
use crate::config::Config;
use crate::error::Error;
use crate::event::Event;
use crate::message::Message;
use crate::transport;
use async_std::stream;
use async_std::sync::{channel, Receiver, Sender};
use behaviour::{Behaviour, BehaviourEventOut};
use futures::select;
use futures_util::stream::StreamExt;
pub use libp2p::gossipsub::{Topic, TopicHash};
use libp2p::identity;
use libp2p::Swarm;
use libp2p::PeerId;
use log::{error, info, trace, warn};
use std::time::Duration;

pub struct Network {
    peer_id: PeerId,
    config: Config,
    swarm: Swarm<Behaviour>,
    message_receiver: Receiver<Message>,
    message_sender: Sender<Message>,
    message_topic: Topic,
    event_receiver: Receiver<Event>,
    event_sender: Sender<Event>,
}

impl Network {
    pub fn new(config: Config) -> Result<Self, Error> {
        // TODO: Read from file (check config for key_file path, if not create on demand)
        let local_key = identity::Keypair::generate_ed25519();
        let local_public = local_key.public();
        let peer_id = local_public.clone().into_peer_id();
        info!("Local node identity is: {}", peer_id.to_base58());


        let transport = transport::build_transport(&local_key);
        let behaviour = Behaviour::new(&local_key, &config);

        let mut swarm = Swarm::new(transport, behaviour, peer_id.clone());

        Swarm::listen_on(&mut swarm, config.listening_multiaddr.clone()).unwrap();

        for to_dial in &config.bootstrap_peers {
            match libp2p::Swarm::dial_addr(&mut swarm, to_dial.clone()) {
                Ok(_) => info!("Dialed {:?}", to_dial),
                Err(e) => error!("Dial {:?} failed: {:?}", to_dial, e),
            }
        }

        // Bootstrap with Kademlia
        if let Err(e) = swarm.bootstrap() {
            warn!("Failed to bootstrap with Kademlia: {}", e);
        }
        let (message_sender, message_receiver) = channel(30);
        let (event_sender, event_receiver) = channel(30);
        let message_topic = Topic::new(format!(
            "/libp2p-boilerplate/{}/message",
            config.network_name
        ));

        swarm.subscribe(message_topic.clone());

        Ok(Network {
            peer_id,
            config,
            swarm,
            message_sender,
            message_receiver,
            message_topic,
            event_sender,
            event_receiver,
        })
    }
    pub fn name(&self) -> String {
        self.config.network_name.clone()
    }

    pub fn peer_id(&self) -> PeerId {
        self.peer_id.clone()
    }

    pub fn message_sender(&self) -> Sender<Message> {
        self.message_sender.clone()
    }

    pub fn message_receiver(&self) -> Receiver<Message> {
        self.message_receiver.clone()
    }

    pub fn event_receiver(&self) -> Receiver<Event> {
        self.event_receiver.clone()
    }
    pub async fn run(self) {
        let mut swarm_stream = self.swarm.fuse();
        let mut message_stream = self.message_receiver.fuse();
        let mut interval = stream::interval(Duration::from_secs(10)).fuse();

        loop {
            select! {
                swarm_event = swarm_stream.next() => match swarm_event {
                    Some(event) => match event {
                        BehaviourEventOut::PeerConnected(peer_id) =>{
                            info!("Peer dialed {:?}", peer_id);
                            self.event_sender.send(Event::PeerConnected(peer_id)).await;
                        }
                        BehaviourEventOut::PeerDisconnected(peer_id) =>{
                            info!("Peer disconnected {:?}", peer_id);
                            self.event_sender.send(Event::PeerDisconnected(peer_id)).await;
                        }
                        BehaviourEventOut::GossipMessage {
                            source,
                            topics,
                            message,
                        } => {
                            trace!("Got a Gossip message from {:?}", source);

                            self.message_sender.send(message).await;
                        }
                        BehaviourEventOut::BitswapReceivedBlock(peer_id, cid, block) => {

                        },
                        BehaviourEventOut::BitswapReceivedWant(peer_id, cid,) => {

                        },
                    }
                    None => { break; }
                },
                message = message_stream.next() => match message {
                    Some(msg) => {
                        if let Err(e) = swarm_stream.get_mut().publish(&self.message_topic, msg) {
                            warn!("Failed to send gossipsub message: {:?}", e);
                        }
                    }
                    None => { break; }
                },
                interval_event = interval.next() => if interval_event.is_some() {
                    trace!("Peers connected: {}", swarm_stream.get_ref().peers().len());
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "./network_test.rs"]
mod network_test;
