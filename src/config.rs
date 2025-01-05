use dotenvy::dotenv;
use serde::Deserialize;
use std::env;
use thiserror::Error;


#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_username: String,
    pub mqtt_password: String,
    pub mqtt_max_retries: i32,
    pub mqtt_retry_interval_ms: u64,

    pub log_topic: String,
    pub status_topic: String,
    pub command_topic: String,
    pub progress_topic: String,
    pub analytics_topic: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Environment variable {0} is missing or invalid.")]
    MissingOrInvalid(String),
    #[error("Parsing error: {0}")]
    ParsingError(String),
}

impl Config {
    /// Validate timeout values and other critical configurations.
    fn validate_timeouts(&self) -> Result<(), ConfigError> {
        const MIN_TIMEOUT: u64 = 100;
        const MAX_TIMEOUT: u64 = 1_000_000;

        if !(MIN_TIMEOUT..=MAX_TIMEOUT).contains(&self.mqtt_retry_interval_ms) {
            return Err(ConfigError::ParsingError(format!(
                "MQTT_RETRY_INTERVAL_MS must be between {} and {} ms",
                MIN_TIMEOUT, MAX_TIMEOUT
            )));
        }

        Ok(())
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        dotenv().ok(); // Load environment variables from .env file

        // Helper to prepend root topic if available
        let prepend_root_topic = |root: &str, topic: &str| {
            if !root.is_empty() {
                format!("{}/{}", root.trim_end_matches('/'), topic.trim_start_matches('/'))
            } else {
                topic.to_string()
            }
        };

        let mqtt_root_topic = env::var("MQTT_ROOT_TOPIC").unwrap_or_else(|_| "image_uploader".to_string());

        let config = Self {
            // MQTT Configuration
            mqtt_host: env::var("MQTT_HOST").map_err(|_| ConfigError::MissingOrInvalid("MQTT_HOST".to_string()))?,
            mqtt_port: env::var("MQTT_PORT")
                .map_err(|_| ConfigError::MissingOrInvalid("MQTT_PORT".to_string()))?
                .parse::<u16>()
                .map_err(|_| ConfigError::ParsingError("MQTT_PORT must be a valid number".to_string()))?,
            mqtt_username: env::var("MQTT_USERNAME").unwrap_or_default(), // Default to empty
            mqtt_password: env::var("MQTT_PASSWORD").unwrap_or_default(), // Default to empty
            mqtt_max_retries: env::var("MQTT_MAX_RETRIES")
                .unwrap_or_else(|_| "-1".to_string())
                .parse::<i32>()
                .map_err(|_| ConfigError::ParsingError("MQTT_MAX_RETRIES must be an integer".to_string()))?,
            mqtt_retry_interval_ms: env::var("MQTT_RETRY_INTERVAL_MS")
                .unwrap_or_else(|_| "5000".to_string())
                .parse::<u64>()
                .map_err(|_| ConfigError::ParsingError("MQTT_RETRY_INTERVAL_MS must be a valid number".to_string()))?,


            // MQTT Topics
            log_topic: prepend_root_topic(&mqtt_root_topic, "/logs"),
            status_topic: prepend_root_topic(&mqtt_root_topic, "/status"),
            command_topic: prepend_root_topic(&mqtt_root_topic, "/commands"),
            progress_topic: prepend_root_topic(&mqtt_root_topic, "/progress"),
            analytics_topic: prepend_root_topic(&mqtt_root_topic, "/analytics"),
        };

        // Validate timeouts after constructing the configuration
        config.validate_timeouts()?;

        Ok(config)
    }
}

