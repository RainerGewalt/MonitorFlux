mod config;
mod mqtt_service;
mod progress_tracker;
mod service_utils;
mod rest_server;
mod db;
mod models;

use crate::config::Config;
use crate::db::DatabaseService;
use crate::mqtt_service::{MqttConfig, MqttService};
use crate::progress_tracker::SharedState;
use crate::rest_server::run_rest_server;
use crate::service_utils::{
    handle_shutdown, periodic_status_update, publish_status, start_logging, start_mqtt_service,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Load configuration
    let config = match Config::from_env() {
        Ok(cfg) => Arc::new(cfg),
        Err(e) => {
            error!("Error loading configuration: {:?}", e);
            return;
        }
    };

    let db_service = match DatabaseService::new("mqtt_storage.db") {
        Ok(service) => Arc::new(service),
        Err(e) => {
            error!("Failed to create database service: {:?}", e);
            return;
        }
    };

    if let Err(e) = db_service.initialize_db() {
        error!("Database initialization failed: {:?}", e);
        return;
    }
    info!("Database initialized successfully.");

    // Broker für internen MQTT-Service überprüfen
    if let Err(e) = db_service.validate_or_add_broker(
        &config.internal_mqtt_host,
        &config.internal_mqtt_host,
        config.internal_mqtt_port,
        Some(&config.internal_mqtt_username),
        Some(&config.internal_mqtt_password),
        config.internal_mqtt_ssl_enabled,
    ) {
        error!("Failed to validate internal broker: {:?}", e);
        return;
    }

    // Brok

    // Shared state for progress tracking
    let state: SharedState = Arc::new(Mutex::new(HashMap::new()));

    let mqtt_service_internal = MqttService::new(
        state.clone(),
        MqttConfig {
            mqtt_host: config.internal_mqtt_host.clone(),
            mqtt_port: config.internal_mqtt_port,
            mqtt_username: config.internal_mqtt_username.clone(),
            mqtt_password: config.internal_mqtt_password.clone(),
            mqtt_ssl_enabled: config.internal_mqtt_ssl_enabled,
            mqtt_ssl_cert_path: config.internal_mqtt_ssl_cert_path.clone(),
            log_topic: config.log_topic.clone(),
            status_topic: config.status_topic.clone(),
            command_topic: config.command_topic.clone(),
            progress_topic: config.progress_topic.clone(),
            analytics_topic: config.analytics_topic.clone(),
            mqtt_max_retries: config.mqtt_max_retries,
            mqtt_retry_interval_ms: config.mqtt_retry_interval_ms,
        },
        None, // Keine Datenbankoperationen für `mqtt_service_internal`
    );

    let mqtt_service_monitored = MqttService::new(
        state.clone(),
        MqttConfig {
            mqtt_host: config.monitored_mqtt_host.clone(),
            mqtt_port: config.monitored_mqtt_port,
            mqtt_username: config.monitored_mqtt_username.clone(),
            mqtt_password: config.monitored_mqtt_password.clone(),
            mqtt_ssl_enabled: config.monitored_mqtt_ssl_enabled,
            mqtt_ssl_cert_path: config.monitored_mqtt_ssl_cert_path.clone(),
            log_topic: config.log_topic.clone(),
            status_topic: config.status_topic.clone(),
            command_topic: config.command_topic.clone(),
            progress_topic: config.progress_topic.clone(),
            analytics_topic: config.analytics_topic.clone(),
            mqtt_max_retries: config.mqtt_max_retries,
            mqtt_retry_interval_ms: config.mqtt_retry_interval_ms,
        },
        Some(db_service.clone()), // Datenbankoperationen für `mqtt_service_monitored`
    );


    // Start both MQTT services
    start_mqtt_service(mqtt_service_internal.clone(), "internal");
    start_mqtt_service(mqtt_service_monitored.clone(), "monitored");

    // Start periodic status updates for both services
    start_logging(mqtt_service_internal.clone(), "Service is starting...".to_string());
    periodic_status_update(mqtt_service_internal.clone(), "internal");

    // Publish startup status for both services
    publish_status(
        mqtt_service_internal.clone(),
        "running".to_string(),
        Some("Internal MQTT service started successfully.".to_string()),
    );

    publish_status(
        mqtt_service_monitored.clone(),
        "running".to_string(),
        Some("Monitored MQTT service started successfully.".to_string()),
    );

    // Start REST API server
    let config_for_rest_api = (*config).clone();
    let rest_api_task = tokio::spawn(async move {
        run_rest_server(db_service, config_for_rest_api).await;
    });

    // Handle shutdown for both MQTT services
    handle_shutdown(mqtt_service_internal.clone(), "internal").await;
    handle_shutdown(mqtt_service_monitored.clone(), "monitored").await;

    // Publish shutdown status for both services
    publish_status(
        mqtt_service_internal.clone(),
        "shutdown".to_string(),
        Some("Internal MQTT service is shutting down.".to_string()),
    );

    publish_status(
        mqtt_service_monitored.clone(),
        "shutdown".to_string(),
        Some("Monitored MQTT service is shutting down.".to_string()),
    );

    // Wait for tasks to complete
    let _ = tokio::join!(rest_api_task);
    info!("All services shut down successfully.");
}
