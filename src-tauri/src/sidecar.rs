//! Manages sidecar backend connections via gRPC.

use sgx_proto::messenger::v1::messenger_service_client::MessengerServiceClient;
use sgx_proto::messenger::v1::*;
use tokio::sync::Mutex;
use tonic::transport::Channel;
use tracing::{error, info, warn};

/// A single sidecar gRPC connection.
pub struct SidecarConnection {
    pub backend_id: String,
    pub client: MessengerServiceClient<Channel>,
}

/// Registry of active sidecar connections.
pub struct SidecarManager {
    connections: Mutex<Vec<SidecarConnection>>,
}

impl SidecarManager {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(Vec::new()),
        }
    }

    /// Connect to a sidecar gRPC server. Retries up to 10 times.
    pub async fn connect(&self, backend_id: &str, port: u16) -> Result<(), String> {
        let addr = format!("http://127.0.0.1:{port}");

        let mut client = None;
        for attempt in 1..=10 {
            match MessengerServiceClient::connect(addr.clone()).await {
                Ok(c) => {
                    info!("Connected to {backend_id} sidecar on port {port}");
                    client = Some(c);
                    break;
                }
                Err(e) => {
                    if attempt == 10 {
                        return Err(format!("Failed to connect to {backend_id}: {e}"));
                    }
                    warn!("Attempt {attempt}/10 to connect to {backend_id} failed, retrying...");
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                }
            }
        }

        let client = client.unwrap();
        let mut conns = self.connections.lock().await;
        conns.retain(|c| c.backend_id != backend_id);
        conns.push(SidecarConnection {
            backend_id: backend_id.to_string(),
            client,
        });

        Ok(())
    }

    /// Get a cloned client for a specific backend.
    pub async fn get_client(&self, backend_id: &str) -> Option<MessengerServiceClient<Channel>> {
        let conns = self.connections.lock().await;
        conns
            .iter()
            .find(|c| c.backend_id == backend_id)
            .map(|c| c.client.clone())
    }

    /// List all connected backend IDs.
    pub async fn list_backends(&self) -> Vec<String> {
        let conns = self.connections.lock().await;
        conns.iter().map(|c| c.backend_id.clone()).collect()
    }

    /// Get merged chat list from all backends.
    pub async fn list_all_chats(&self, limit: i32) -> Result<Vec<Chat>, String> {
        let conns = self.connections.lock().await;
        let mut all_chats: Vec<Chat> = Vec::new();

        for conn in conns.iter() {
            let mut client = conn.client.clone();
            match client
                .list_chats(ListChatsRequest {
                    limit,
                    offset_order: 0,
                })
                .await
            {
                Ok(response) => {
                    all_chats.extend(response.into_inner().chats);
                }
                Err(e) => {
                    error!("Failed to list chats from {}: {e}", conn.backend_id);
                }
            }
        }

        all_chats.sort_by(|a, b| {
            let ta = a.last_activity.as_ref().map(|t| t.seconds).unwrap_or(0);
            let tb = b.last_activity.as_ref().map(|t| t.seconds).unwrap_or(0);
            tb.cmp(&ta)
        });

        Ok(all_chats)
    }

    /// Get backend info from all connections.
    pub async fn get_all_backend_info(&self) -> Vec<BackendInfo> {
        let conns = self.connections.lock().await;
        let mut infos = Vec::new();

        for conn in conns.iter() {
            let mut client = conn.client.clone();
            if let Ok(response) = client.get_backend_info(GetBackendInfoRequest {}).await {
                infos.push(response.into_inner());
            }
        }

        infos
    }
}
