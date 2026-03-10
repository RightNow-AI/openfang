use crate::protocol::{A2AMessage, A2APayload, TaskRequest, TaskResponse, TaskStatus};
use crate::transport::A2ATransport;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct A2AEngine {
    transport: Arc<Mutex<dyn A2ATransport>>,
    pending_tasks: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<TaskResponse>>>>,
}

impl A2AEngine {
    pub fn new(transport: Arc<Mutex<dyn A2ATransport>>) -> Self {
        Self {
            transport,
            pending_tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start(&self) {
        let transport = self.transport.clone();
        let pending_tasks = self.pending_tasks.clone();

        tokio::spawn(async move {
            loop {
                let mut transport = transport.lock().await;
                if let Some(message) = transport.receive().await {
                    match message.payload {
                        A2APayload::TaskResponse(response) => {
                            let mut pending_tasks = pending_tasks.lock().await;
                            if let Some(sender) = pending_tasks.remove(&response.task_id) {
                                sender.send(response).unwrap();
                            }
                        }
                        _ => {}
                    }
                }
            }
        });
    }

    pub async fn send_task(&self, task_request: TaskRequest) -> TaskResponse {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let task_id = task_request.task_id.clone();

        {
            let mut pending_tasks = self.pending_tasks.lock().await;
            pending_tasks.insert(task_id.clone(), sender);
        }

        let message = A2AMessage {
            version: "1.0".to_string(),
            message_id: uuid::Uuid::new_v4().to_string(),
            sender_id: "self".to_string(),
            receiver_id: "other".to_string(),
            payload: A2APayload::TaskRequest(task_request),
        };

        self.transport.lock().await.send(message).await;

        receiver.await.unwrap()
    }
}
