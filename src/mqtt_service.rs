use crate::progress_tracker::{ProgressTracker, SharedState};
use log::{debug, error, info, warn};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use serde::Deserialize;
use serde_json;
use uuid::Uuid;


#[derive(Deserialize, Debug, Clone)]
pub struct UploadRequest {
    pub action: String,                    // z. B. "start" oder "stop"
    pub options: Option<UploadOptions>,   // Optional, da "stop" keine Optionen benötigt
}

#[derive(Deserialize, Debug, Clone)]
pub struct UploadOptions {
    pub upload_type: Option<String>,              // Optional für "stop"
    pub task_id: Option<String>,                  // ID des zu stoppenden Tasks
    pub recursive_folders: Option<Vec<FolderConfig>>,
    pub files: Option<Vec<FileDetail>>,
    pub compression: Option<CompressionConfig>,
    pub file_filters: Option<Vec<String>>,
    pub upload_strategy: Option<String>,          // z. B. "batch" oder "sequential"
}


#[derive(Deserialize, Debug, Clone)]
pub struct FolderConfig {
    pub path: String,
    pub recursive: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FileDetail {
    pub source_path: String,
    pub destination_path: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CompressionConfig {
    pub enabled: bool,
    pub quality: u8, // Qualität (0–9)
}



#[derive(Debug)]
enum ClientState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}
use crate::config::Config;

pub struct MqttService {
    client_state: Mutex<ClientState>,
    client: Mutex<Option<AsyncClient>>,
    state: SharedState,  // Root topic for MQTT messages
    pub(crate) config: Config,
}

impl MqttService {
    pub fn new(state: SharedState, config: Config) -> Arc<Self> {
        Arc::new(Self {
            client_state: Mutex::new(ClientState::Disconnected),
            client: Mutex::new(None),
            state,
            config
        })
    }
    pub async fn start(self: Arc<Self>, mqtt_host: &str, mqtt_port: u16, mqtt_client_id: &str) {
        info!("Starting MQTT service...");

        let initial_retry_interval = Duration::from_millis(self.config.mqtt_retry_interval_ms);
        let max_retries = std::cmp::min(if self.config.mqtt_max_retries > 0 {
            self.config.mqtt_max_retries
        } else {
            5
        }, 100); // Default to 5, cap at 100
        let mut retry_interval = initial_retry_interval;
        let mut retries = 0;

        loop {
            if max_retries != -1 && retries >= max_retries {
                error!("Maximum number of retries ({}) reached. Stopping the service.", max_retries);
                break;
            }

            debug!("Configuring MQTT broker at {}:{}...", mqtt_host, mqtt_port);

            let mut mqtt_options = MqttOptions::new(mqtt_client_id, mqtt_host, mqtt_port);
            mqtt_options.set_keep_alive(Duration::from_secs(10));
            mqtt_options.set_clean_session(true);

            if !self.config.mqtt_username.is_empty() && !self.config.mqtt_password.is_empty() {
                mqtt_options.set_credentials(
                    &self.config.mqtt_username,
                    &self.config.mqtt_password,
                );
            }

            let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10);

            {
                let mut client_lock = self.client.lock().await;
                *client_lock = Some(client.clone());
            }

            {
                let mut client_state = self.client_state.lock().await;
                *client_state = ClientState::Connecting;
            }

            let control_topic = self.config.command_topic.clone();
            match client.subscribe(&control_topic, QoS::AtLeastOnce).await {
                Ok(_) => {
                    info!("Successfully subscribed to topic '{}'.", control_topic);
                    {
                        let mut client_state = self.client_state.lock().await;
                        *client_state = ClientState::Connected;
                    }
                    retry_interval = initial_retry_interval;
                }
                Err(e) => {
                    error!("Failed to subscribe to topic '{}': {}", control_topic, e);
                    {
                        let mut client_state = self.client_state.lock().await;
                        *client_state = ClientState::Error(e.to_string());
                    }
                    retries += 1;
                    sleep(retry_interval).await;
                    retry_interval = (retry_interval * 2).min(Duration::from_secs(60));
                    continue;
                }
            }

            loop {
                match eventloop.poll().await {
                    Ok(event) => {
                        let self_clone = self.clone();
                        tokio::spawn(async move {
                            self_clone.handle_event(event).await;
                        });
                    }
                    Err(e) => {
                        error!("Error in MQTT event loop: {:?}", e);
                        {
                            let mut client_state = self.client_state.lock().await;
                            *client_state = ClientState::Disconnected;
                        }
                        break;
                    }
                }
            }

            warn!(
            "Lost connection to MQTT broker. Retrying in {:?}...",
            retry_interval
        );
            retries += 1;
            sleep(retry_interval).await;
            retry_interval = (retry_interval * 2).min(Duration::from_secs(60));
        }
    }







    async fn handle_event(self: Arc<Self>, event: Event) {
        match event {
            Event::Incoming(Packet::Publish(publish)) => {
                let topic = publish.topic.clone();
                let payload =
                    String::from_utf8(publish.payload.to_vec()).unwrap_or_else(|_| "".to_string());

                let control_topic = self.config.command_topic.clone();
                if topic == control_topic {
                    debug!("Incoming control-event.");
                } else {
                    warn!("Unknown topic received: {}", topic);
                }

            }
            Event::Incoming(Packet::ConnAck(_)) => {
                info!("Connected to MQTT broker.");
            }
            Event::Outgoing(_) => {
                debug!("Outgoing event.");
            }
            _ => {
                debug!("Unhandled event: {:?}", event);
            }
        }
    }
    pub async fn publish_message(
        &self,
        topic: &str, // Direktes Topic ohne Root-Topic
        message: &str,
        qos: QoS,
        retain: bool,
    ) {
        for _ in 0..5 { // Retry up to 5 times
            let client = self.client.lock().await;
            if let Some(client) = client.as_ref() {
                // Das Root-Topic ist bereits in der Konfiguration enthalten
                let full_topic = topic.to_string();

                match client.publish(full_topic.clone(), qos, retain, message).await {
                    Ok(_) => {
                        info!("Message published to '{}': {}", full_topic, message);
                        return;
                    }
                    Err(e) => {
                        error!("Failed to publish message to '{}': {:?}", full_topic, e);
                    }
                }
            } else {
                error!("MQTT client is not connected. Retrying...");
            }

            // Wait and retry
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        error!(
        "Failed to publish message to topic '{}' after multiple retries: {}",
        topic, message
    );
    }



}
