use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::ids::AgentName;

#[cfg(test)]
use std::io::Read;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aAgentCard {
    pub name: AgentName,
    pub endpoint: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct A2aMessage {
    pub task_id: String,
    pub role: String,
    pub text: String,
}

#[async_trait]
pub trait A2aTransport: Send + Sync {
    async fn send_message(
        &self,
        card: &A2aAgentCard,
        message: A2aMessage,
    ) -> Result<A2aMessage, A2aError>;
}

#[derive(Clone)]
pub struct RemoteA2aAgent<T: A2aTransport> {
    pub card: A2aAgentCard,
    pub transport: T,
}

impl<T: A2aTransport> RemoteA2aAgent<T> {
    pub async fn invoke(&self, message: A2aMessage) -> Result<A2aMessage, A2aError> {
        self.transport.send_message(&self.card, message).await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum A2aError {
    #[error("A2A transport failed: {0}")]
    Transport(String),
}

/// HTTP-based transport for A2A (agent-to-agent) communication.
/// 
/// This transport POSTs serialized `A2aMessage` (JSON) to a remote agent's endpoint
/// and deserializes the JSON response back into an `A2aMessage`.
#[derive(Clone)]
pub struct HttpA2aTransport {
    client: reqwest::Client,
}

impl HttpA2aTransport {
    /// Creates a new HTTP A2A transport.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for HttpA2aTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl A2aTransport for HttpA2aTransport {
    async fn send_message(
        &self,
        card: &A2aAgentCard,
        message: A2aMessage,
    ) -> Result<A2aMessage, A2aError> {
        let response = self
            .client
            .post(&card.endpoint)
            .json(&message)
            .send()
            .await
            .map_err(|e| A2aError::Transport(format!("HTTP request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "[unable to read body]".to_string());
            return Err(A2aError::Transport(format!(
                "HTTP {} response: {}",
                status, body
            )));
        }

        response
            .json::<A2aMessage>()
            .await
            .map_err(|e| A2aError::Transport(format!("Failed to deserialize response: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    #[tokio::test]
    async fn test_a2a_http_transport_send_message() {
        // Spin up a fake HTTP server on 127.0.0.1:0
        let listener = TcpListener::bind("127.0.0.1:0")
            .expect("failed to bind to localhost");
        let addr = listener.local_addr().expect("failed to get local addr");
        let endpoint = format!("http://{}/agent", addr);

        // Store received message to verify
        let received_msg = Arc::new(Mutex::new(None));
        let received_msg_clone = Arc::clone(&received_msg);

        // Spawn server thread
        let _server_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(mut stream) = stream {
                    let mut buffer = [0; 4096];
                    if let Ok(n) = stream.read(&mut buffer) {
                        let request = String::from_utf8_lossy(&buffer[..n]);

                        // Parse JSON from POST body (simple extraction)
                        if let Some(json_start) = request.rfind('{') {
                            let json_part = &request[json_start..];
                            if let Ok(msg) = serde_json::from_str::<A2aMessage>(json_part) {
                                *received_msg_clone.lock().unwrap() = Some(msg);
                            }
                        }

                        // Send HTTP response
                        let response_msg = A2aMessage {
                            task_id: "resp-task-1".to_string(),
                            role: "agent".to_string(),
                            text: "pong".to_string(),
                        };
                        let response_json = serde_json::to_string(&response_msg).unwrap();
                        let http_response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                            response_json.len(),
                            response_json
                        );
                        let _ = stream.write_all(http_response.as_bytes());
                    }
                    break;
                }
            }
        });

        // Give server a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Create transport and message
        let transport = HttpA2aTransport::new();
        let card = A2aAgentCard {
            name: AgentName::new("test-agent").unwrap(),
            endpoint: endpoint.clone(),
            capabilities: vec!["chat".to_string()],
        };
        let message = A2aMessage {
            task_id: "task-1".to_string(),
            role: "user".to_string(),
            text: "ping".to_string(),
        };

        // Send message
        let result = transport.send_message(&card, message.clone()).await;

        // Verify success
        assert!(result.is_ok(), "send_message failed: {:?}", result);
        let response = result.unwrap();
        assert_eq!(response.task_id, "resp-task-1");
        assert_eq!(response.role, "agent");
        assert_eq!(response.text, "pong");

        // Verify server received our message
        let received = received_msg.lock().unwrap();
        assert!(received.is_some(), "server did not receive message");
        let recv_msg = received.as_ref().unwrap();
        assert_eq!(recv_msg.task_id, "task-1");
        assert_eq!(recv_msg.role, "user");
        assert_eq!(recv_msg.text, "ping");
    }

    #[tokio::test]
    async fn test_a2a_http_transport_error_handling() {
        // Create a transport with a non-existent endpoint
        let transport = HttpA2aTransport::new();
        let card = A2aAgentCard {
            name: AgentName::new("test-agent").unwrap(),
            endpoint: "http://127.0.0.1:1/nonexistent".to_string(),
            capabilities: vec![],
        };
        let message = A2aMessage {
            task_id: "task-err".to_string(),
            role: "user".to_string(),
            text: "test".to_string(),
        };

        // Should fail
        let result = transport.send_message(&card, message).await;
        assert!(result.is_err(), "expected error for unreachable endpoint");
        match result {
            Err(A2aError::Transport(msg)) => {
                assert!(msg.contains("failed"), "error message should describe failure");
            }
            _ => panic!("expected A2aError::Transport"),
        }
    }

    #[tokio::test]
    async fn test_a2a_remote_agent_invoke() {
        // Spin up a fake HTTP server
        let listener = TcpListener::bind("127.0.0.1:0")
            .expect("failed to bind to localhost");
        let addr = listener.local_addr().expect("failed to get local addr");
        let endpoint = format!("http://{}/invoke", addr);

        // Spawn server thread
        let _server_thread = thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(mut stream) = stream {
                    let mut buffer = [0; 4096];
                    if let Ok(_n) = stream.read(&mut buffer) {
                        let response_msg = A2aMessage {
                            task_id: "task-response".to_string(),
                            role: "assistant".to_string(),
                            text: "response from remote".to_string(),
                        };
                        let response_json = serde_json::to_string(&response_msg).unwrap();
                        let http_response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                            response_json.len(),
                            response_json
                        );
                        let _ = stream.write_all(http_response.as_bytes());
                    }
                    break;
                }
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Create RemoteA2aAgent with HttpA2aTransport
        let transport = HttpA2aTransport::new();
        let card = A2aAgentCard {
            name: AgentName::new("remote").unwrap(),
            endpoint,
            capabilities: vec!["test".to_string()],
        };
        let agent = RemoteA2aAgent { card, transport };

        let message = A2aMessage {
            task_id: "req".to_string(),
            role: "user".to_string(),
            text: "hello remote".to_string(),
        };

        let result = agent.invoke(message).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.text, "response from remote");
    }
}
