use crate::protocol::A2AMessage;
use async_trait::async_trait;
use tokio::sync::mpsc;

#[async_trait]
pub trait A2ATransport: Send + Sync {
    async fn send(&self, message: A2AMessage);
    async fn receive(&mut self) -> Option<A2AMessage>;
}

pub struct MpscTransport {
    sender: mpsc::Sender<A2AMessage>,
    receiver: mpsc::Receiver<A2AMessage>,
}

impl MpscTransport {
    pub fn new() -> (Self, Self) {
        let (sender1, receiver1) = mpsc::channel(100);
        let (sender2, receiver2) = mpsc::channel(100);

        let transport1 = Self {
            sender: sender1,
            receiver: receiver2,
        };

        let transport2 = Self {
            sender: sender2,
            receiver: receiver1,
        };

        (transport1, transport2)
    }
}

#[async_trait]
impl A2ATransport for MpscTransport {
    async fn send(&self, message: A2AMessage) {
        self.sender.send(message).await.unwrap();
    }

    async fn receive(&mut self) -> Option<A2AMessage> {
        self.receiver.recv().await
    }
}
