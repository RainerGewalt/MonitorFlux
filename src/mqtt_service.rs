use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS, Transport, TlsConfiguration};
use std::fs::read;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use log::{debug, error, info, warn};

use crate::db::DatabaseService;
use crate::progress_tracker::SharedState;

#[derive(Debug)]
enum ClientState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}
#[derive(Debug, Clone)]
pub struct MqttConfig {
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_username: String,
    pub mqtt_password: String,
    pub mqtt_ssl_enabled: bool,
    pub mqtt_ssl_cert_path: Option<String>,
    pub log_topic: String,
    pub status_topic: String,
    pub command_topic: String,
    pub progress_topic: String,
    pub analytics_topic: String,
    pub mqtt_max_retries: i32,
    pub mqtt_retry_interval_ms: u64,
}

pub struct MqttService {
    client_state: Mutex<ClientState>,
    client: Mutex<Option<AsyncClient>>,
    state: SharedState,
    pub config: MqttConfig,
}

impl MqttService {
    pub fn new(state: SharedState, config: MqttConfig) -> Arc<Self> {
        Arc::new(Self {
            client_state: Mutex::new(ClientState::Disconnected),
            client: Mutex::new(None),
            state,
            config,
        })
    }

    pub async fn start(self: Arc<Self>, mqtt_host: &str, mqtt_port: u16, mqtt_client_id: &str) {
        info!("Starting MQTT service...");

        let initial_retry_interval = Duration::from_millis(self.config.mqtt_retry_interval_ms);
        let max_retries = if self.config.mqtt_max_retries > 0 {
            self.config.mqtt_max_retries
        } else {
            -1 // -1 = unendlich viele Versuche
        };
        let mut retry_interval = initial_retry_interval;
        let mut retries = 0;

        loop {
            if max_retries != -1 && retries >= max_retries {
                error!(
                    "Maximum number of retries ({}) reached. Stopping the service.",
                    max_retries
                );
                break;
            }

            debug!("Configuring MQTT broker at {}:{}...", mqtt_host, mqtt_port);
            let mut mqtt_options = MqttOptions::new(mqtt_client_id, mqtt_host, mqtt_port);
            mqtt_options.set_keep_alive(Duration::from_secs(10));
            mqtt_options.set_clean_session(true);

            // Optional: Benutzername/Passwort setzen
            if !self.config.mqtt_username.is_empty() && !self.config.mqtt_password.is_empty() {
                mqtt_options.set_credentials(
                    &self.config.mqtt_username,
                    &self.config.mqtt_password,
                );
            }

            // TLS aktivieren
            if self.config.mqtt_ssl_enabled {
                if let Some(cert_path) = &self.config.mqtt_ssl_cert_path {
                    match read(cert_path) {
                        Ok(ca) => {
                            // Wichtig: TlsConfiguration::Simple verlangt ein `Vec<u8>` in `ca`,
                            // KEIN `Option<Vec<u8>>`. `alpn` und `client_auth` sind optional.
                            let tls_config = TlsConfiguration::Simple {
                                ca,                 // hier der geladene CA-Bytestring
                                alpn: None,         // z. B. Some(vec![b"h2".to_vec(), b"http/1.1".to_vec()])
                                client_auth: None,  // hier könntest du Client-Zertifikat + Key eintragen
                            };
                            mqtt_options.set_transport(Transport::tls_with_config(tls_config));
                            info!("Using TLS with CA certificate from: {}", cert_path);
                        }
                        Err(e) => {
                            error!(
                                "Failed to load CA certificate from '{}': {}. Stopping service.",
                                cert_path, e
                            );
                            break;
                        }
                    }
                } else {
                    error!("MQTT_SSL_ENABLED is true, but no MQTT_SSL_CERT_PATH is provided.");
                    break;
                }
            }

            // AsyncClient + EventLoop erzeugen
            let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10);

            // Client im Mutex hinterlegen
            {
                let mut client_lock = self.client.lock().await;
                *client_lock = Some(client.clone());
            }

            {
                let mut client_state = self.client_state.lock().await;
                *client_state = ClientState::Connecting;
            }

            // Subscriben
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

            // MQTT-Event-Loop
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
                        break; // Verlasse die innere Schleife => Reconnect
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

                match DatabaseService::new("mqtt_storage.db") {
                    Ok(db_service) => {
                        if let Err(e) = db_service.insert_value(&topic, &payload) {
                            error!("Failed to insert value for topic '{}': {:?}", topic, e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to initialize DatabaseService: {:?}", e);
                    }
                }
            }
            Event::Incoming(Packet::ConnAck(_)) => {
                info!("Connected to MQTT broker.");

                // Abonniere alle Topics
                if let Some(client) = self.client.lock().await.as_ref() {
                    if let Err(e) = client.subscribe("#", QoS::AtMostOnce).await {
                        error!("Failed to subscribe to wildcard topic: {:?}", e);
                    } else {
                        info!("Successfully subscribed to all topics.");
                    }
                }
            }
            _ => {
                debug!("Unhandled event: {:?}", event);
            }
        }
    }
    pub async fn publish_message(
        &self,
        topic: &str,
        message: &str,
        qos: QoS,
        retain: bool,
    ) {
        // Mehrfache Publish-Versuche (simple Retry-Logik)
        for _ in 0..5 {
            let client = self.client.lock().await;
            if let Some(client) = client.as_ref() {
                // Falls das Topic bereits in der Config zusammengebaut wird,
                // hier nur noch `topic.to_string()` verwenden
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

            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        error!(
            "Failed to publish message to topic '{}' after multiple retries: {}",
            topic, message
        );
    }
}
