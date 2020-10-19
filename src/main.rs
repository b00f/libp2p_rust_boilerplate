use log::LevelFilter;
use network::Config;
use network::Message;
use network::Network;
use simple_logger::SimpleLogger;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{thread, time::Duration};

#[tokio::main]
async fn main() {
    SimpleLogger::new()
        .with_module_level("*", LevelFilter::Off)
        .with_level(LevelFilter::Off)
        .init()
        .expect("unable to start logger");

    let running = Arc::new(AtomicBool::new(true));

    let conf = Config::default();
    let network = Network::new(conf).unwrap();

    let message_sender = network.message_sender();
    let message_receiver = network.message_receiver();
    let event_receiver = network.event_receiver();

    let network_task = tokio::task::spawn(async {
        network.run().await;
    });

    // TODO: WHY??
    let delay = Duration::from_millis(2000);
    thread::sleep(delay);

    tokio::task::spawn(async move {
        loop {
            if let Ok(msg) = message_receiver.recv().await {
                println!("A message received: {:?}", msg);
            }
        }
    });

    tokio::task::spawn(async move {
        loop {
            if let Ok(event) = event_receiver.recv().await {
                match event {
                    network::Event::PeerConnected(peer_id) => {
                        let msg = Message::Greeting(format!("Hello {}", peer_id));
                        message_sender.send(msg).await;
                    }
                    network::Event::PeerDisconnected(peer_id) => {
                        let msg = Message::Greeting(format!("Goodbye {}", peer_id));
                        message_sender.send(msg).await;
                    }
                }
            }
        }
    });

    while running.load(Ordering::SeqCst) {}

    network_task.abort();
}
