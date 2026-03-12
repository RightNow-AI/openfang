//! A2A Engine - Core runtime for agent-to-agent communication.
//!
//! Manages task requests/responses and coordinates message passing between agents.

use crate::error::A2AError;
use crate::protocol::{A2AMessage, A2APayload, TaskRequest, TaskResponse};
use crate::transport::A2ATransport;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, warn};

/// Engine for A2A communication.
///
/// Handles sending task requests and receiving responses asynchronously.
pub struct A2AEngine {
    transport: Arc<Mutex<dyn A2ATransport>>,
    pending_tasks: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<TaskResponse>>>>,
}

impl A2AEngine {
    /// Create a new A2A engine with the given transport.
    pub fn new(transport: Arc<Mutex<dyn A2ATransport>>) -> Self {
        Self {
            transport,
            pending_tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start the engine's message processing loop.
    ///
    /// This spawns a background task that continuously receives messages
    /// and routes responses to waiting task handles.
    pub async fn start(&self) {
        let transport = self.transport.clone();
        let pending_tasks = self.pending_tasks.clone();

        tokio::spawn(async move {
            loop {
                let mut transport_guard = transport.lock().await;
                if let Some(message) = transport_guard.receive().await {
                    if let A2APayload::TaskResponse(response) = message.payload {
                        let mut pending = pending_tasks.lock().await;
                        if let Some(sender) = pending.remove(&response.task_id) {
                            // Send the response - if receiver is dropped, just log and continue
                            match sender.send(response) {
                                Ok(()) => debug!("Task response sent"),
                                Err(_) => warn!("Task response receiver dropped"),
                            }
                        }
                    }
                }
            }
        });
    }

    /// Send a task request and wait for the response.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The transport fails to send the message
    /// - The response channel is closed
    /// - No response is received
    pub async fn send_task(&self, task_request: TaskRequest) -> Result<TaskResponse, A2AError> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let task_id = task_request.task_id.clone();

        // Register the pending task
        {
            let mut pending_tasks = self.pending_tasks.lock().await;
            pending_tasks.insert(task_id.clone(), sender);
        }

        // Create and send the message
        let message = A2AMessage {
            version: "1.0".to_string(),
            message_id: uuid::Uuid::new_v4().to_string(),
            sender_id: "self".to_string(),
            receiver_id: "other".to_string(),
            payload: A2APayload::TaskRequest(task_request),
        };

        {
            let transport = self.transport.lock().await;
            transport.send(message).await?;
        }

        // Wait for the response
        receiver.await.map_err(|_| {
            warn!("Task response channel closed for task: {}", task_id);
            A2AError::TaskChannelClosed(task_id)
        })
    }
}