mod config;
mod mqtt_service;
mod progress_tracker;
mod service_utils;
mod rest_server;
mod db;
mod models;

use crate::config::Config;
use crate::db::DatabaseService;
use crate::mqtt_service::MqttService;
use crate::progress_tracker::SharedState;
use crate::rest_server::run_rest_server;
use crate::service_utils::{handle_shutdown, periodic_status_update, publish_status, start_logging, start_mqtt_service};
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

    // Initialize the database
    let db_service = match DatabaseService::new("mqtt_storage.db") {
        Ok(service) => service,
        Err(e) => {
            error!("Failed to create database service: {:?}", e);
            return;
        }
    };

    if let Err(e) = db_service.initialize_db() {
        error!("Failed to initialize database: {:?}", e);
        return;
    }
    info!("Database initialized successfully.");

    // Shared state for progress tracking
    let state: SharedState = Arc::new(Mutex::new(HashMap::new()));

    // Create MQTT service
    let mqtt_service = MqttService::new(state.clone(), (*config).clone());

    // Clone `mqtt_service` for use in tasks
    let mqtt_service_clone_for_task = mqtt_service.clone();
    let mqtt_service_clone_for_logging = mqtt_service.clone();
    let mqtt_service_clone_for_status = mqtt_service.clone();

    // Start MQTT service
    let mqtt_service_task = tokio::spawn(async move {
        start_mqtt_service(mqtt_service_clone_for_task);
    });

    // Start REST API server
    let rest_api_task = tokio::spawn(async move {
        run_rest_server(db_service).await;
    });

    // Start periodic logging and updates
    start_logging(mqtt_service_clone_for_logging, "Service is starting...".to_string());
    periodic_status_update(mqtt_service_clone_for_status.clone());

    // Log start status
    publish_status(
        mqtt_service.clone(),
        "running".to_string(),
        Some("Uploader service has started successfully.".to_string()),
    );

    // Handle shutdown
    handle_shutdown(mqtt_service.clone()).await;

    // Log shutdown status
    publish_status(
        mqtt_service.clone(),
        "shutdown".to_string(),
        Some("Uploader service is shutting down.".to_string()),
    );

    // Wait for tasks to complete
    let _ = tokio::join!(mqtt_service_task, rest_api_task);
}

