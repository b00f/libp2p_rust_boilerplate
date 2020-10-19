mod tests {
    use super::super::*;
    use async_std::task;
    use std::{thread, time::Duration};
    //use simple_logger::SimpleLogger;


    fn initialize() -> Result<Network, Error> {
        let conf = Config::default();
        Network::new(conf)
    }

    #[test]
    fn network_initialize() {
        let net = self::initialize();
        assert!(net.is_ok(), "Network initialization failed");
    }

    #[tokio::test(threaded_scheduler)]
    async fn network_discovery() {
        //SimpleLogger::new().init().unwrap();
        let net1 = self::initialize().unwrap();
        let net2 = self::initialize().unwrap();

        let peer_id1 = net1.peer_id();
        let peer_id2 = net2.peer_id();
        let net1_message_sender = net1.message_sender();
        let net2_message_receiver = net2.message_receiver();
        let net1_event_receiver = net1.event_receiver();
        let net2_event_receiver = net2.event_receiver();

        task::spawn(async {
            net1.run().await;
        });

        task::spawn(async {
            net2.run().await;
        });

        let event1 =  net1_event_receiver.recv().await.unwrap();
        assert_eq!(event1, Event::PeerConnected(peer_id2));

        let event2 =  net2_event_receiver.recv().await.unwrap();
        assert_eq!(event2, Event::PeerConnected(peer_id1));

        // TODO: WHY??
        let delay = Duration::from_millis(1000);
        thread::sleep(delay);

        let msg = Message::Greeting("hello".to_owned());
        net1_message_sender.send(msg.clone()).await;

        let msg2 = net2_message_receiver.recv().await;
        assert!(msg2.is_ok(), "Receiver failed");
        assert_eq!(msg, msg2.unwrap());
    }
}
