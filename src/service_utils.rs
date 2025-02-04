use uuid::Uuid;
use std::sync::Arc;
use tracing::{error, info};
use crate::mqtt_service::MqttService;

/// Start an MQTT service with a specific client ID prefix
pub fn start_mqtt_service(mqtt_service: Arc<MqttService>, client_id_prefix: &str) {
    let mqtt_host = mqtt_service.config.mqtt_host.clone();
    let mqtt_port = mqtt_service.config.mqtt_port;
    let mqtt_client_id = format!("{}_{}", client_id_prefix, Uuid::new_v4());

    let mqtt_service_clone = mqtt_service.clone();
    tokio::spawn(async move {
        mqtt_service_clone
            .start(&mqtt_host, mqtt_port, &mqtt_client_id)
            .await;
    });
}

/// Start logging for a specific MQTT service
pub fn start_logging(mqtt_service: Arc<MqttService>, message: String) {
    let mqtt_service_clone = mqtt_service.clone();
    tokio::spawn(async move {
        mqtt_service_clone
            .publish_message(
                &mqtt_service_clone.config.log_topic,
                &format!("{{\"level\": \"INFO\", \"message\": \"{}\"}}", message),
                rumqttc::QoS::AtLeastOnce,
                true,
            )
            .await;
    });
}

/// Publish analytics events for a specific MQTT service
pub fn publish_analytics(
    mqtt_service: Arc<MqttService>,
    event: String,
    details: String
) {
    let mqtt_service_clone = mqtt_service.clone();
    tokio::spawn(async move {
        mqtt_service_clone
            .publish_message(
                &mqtt_service_clone.config.analytics_topic,
                &format!("{{\"event\": \"{}\", \"details\": \"{}\"}}", event, details),
                rumqttc::QoS::AtLeastOnce,
                true,
            )
            .await;
    });
}

/// Publish progress updates for a specific MQTT service
pub fn publish_progress(
    mqtt_service: Arc<MqttService>,
    progress: u64,
    total: u64
) {
    let mqtt_service_clone = mqtt_service.clone();
    let topic = mqtt_service_clone.config.progress_topic.clone();
    tokio::spawn(async move {
        mqtt_service_clone
            .publish_message(
                &topic,
                &format!(
                    "{{\"progress\": {}, \"total\": {}, \"percentage\": {:.2}}}",
                    progress,
                    total,
                    (progress as f64 / total as f64) * 100.0
                ),
                rumqttc::QoS::AtLeastOnce,
                true,
            )
            .await;
    });
}

/// Publish a status update for a specific MQTT service
pub fn publish_status(
    mqtt_service: Arc<MqttService>,
    status: String,
    details: Option<String>
) {
    let mqtt_service_clone = mqtt_service.clone();
    let topic = mqtt_service_clone.config.status_topic.clone();
    let details_message = details.unwrap_or_default();
    tokio::spawn(async move {
        mqtt_service_clone
            .publish_message(
                &topic,
                &format!(
                    "{{\"status\": \"{}\", \"details\": \"{}\"}}",
                    status, details_message
                ),
                rumqttc::QoS::AtLeastOnce,
                true,
            )
            .await;
    });
}

/// Graceful shutdown for a specific MQTT service
pub async fn handle_shutdown(mqtt_service: Arc<MqttService>, client_name: &str) {
    let status_topic = mqtt_service.config.status_topic.clone();

    if let Err(e) = tokio::signal::ctrl_c().await {
        error!("[{}] Failed to handle termination signal: {:?}", client_name, e);

        mqtt_service
            .publish_message(
                &status_topic,
                &format!(
                    "{{\"status\": \"error\", \"message\": \"Termination signal failed for {}\"}}",
                    client_name
                ),
                rumqttc::QoS::AtLeastOnce,
                true,
            )
            .await;
    } else {
        mqtt_service
            .publish_message(
                &status_topic,
                &format!(
                    "{{\"status\": \"shutdown\", \"message\": \"{} is shutting down...\"}}",
                    client_name
                ),
                rumqttc::QoS::AtLeastOnce,
                true,
            )
            .await;

        info!("[{}] is shutting down...", client_name);
    }
}

/// Start periodic status updates for a specific MQTT service
pub fn periodic_status_update(mqtt_service: Arc<MqttService>, client_name: &str) {
    let topic = mqtt_service.config.status_topic.clone();
    let client_name = client_name.to_string(); // Kopiere `client_name` in einen String

    tokio::spawn(async move {
        loop {
            mqtt_service
                .publish_message(
                    &topic,
                    &format!(
                        "{{\"status\": \"running\", \"message\": \"{} is operational\"}}",
                        client_name
                    ),
                    rumqttc::QoS::AtLeastOnce,
                    true,
                )
                .await;

            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        }
    });
}

/// Start multiple MQTT services
pub fn start_multiple_mqtt_services(services: Vec<(Arc<MqttService>, &str)>) {
    for (mqtt_service, client_name) in services {
        start_mqtt_service(mqtt_service.clone(), client_name);
        periodic_status_update(mqtt_service, client_name);
    }
}
