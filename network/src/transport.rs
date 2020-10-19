use libp2p::{
    core,
    core::muxing::StreamMuxerBox,
    core::transport::boxed::Boxed,
    identity::{Keypair},
    mplex, noise, yamux, PeerId, Transport,
};
use std::io::{Error, ErrorKind};
use std::time::Duration;

/// Builds the transport stack that LibP2P will communicate over
pub fn build_transport(local_key: &Keypair) -> Boxed<(PeerId, StreamMuxerBox), Error> {
    let transport = libp2p::tcp::TcpConfig::new().nodelay(true);
    let transport = libp2p::dns::DnsConfig::new(transport).unwrap();
    let dh_keys = noise::Keypair::<noise::X25519Spec>::new()
        .into_authentic(&local_key)
        .expect("Noise key generation failed");
    let mut yamux_config = yamux::Config::default();
    yamux_config.set_max_buffer_size(1 << 20);
    yamux_config.set_receive_window(1 << 20);
    transport
        .upgrade(core::upgrade::Version::V1)
        .authenticate(noise::NoiseConfig::xx(dh_keys).into_authenticated())
        .multiplex(core::upgrade::SelectUpgrade::new(
            yamux_config,
            mplex::MplexConfig::new(),
        ))
        .map(|(peer, muxer), _| (peer, core::muxing::StreamMuxerBox::new(muxer)))
        .timeout(Duration::from_secs(20))
        .map_err(|err| Error::new(ErrorKind::Other, err))
        .boxed()
}