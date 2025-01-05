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
    pub mqtt_ssl_enabled: bool,
    pub mqtt_ssl_cert_path: Option<String>,
    pub mqtt_max_retries: i32,
    pub mqtt_retry_interval_ms: u64,

    pub log_topic: String,
    pub status_topic: String,
    pub command_topic: String,
    pub progress_topic: String,
    pub analytics_topic: String,

    pub rest_api_host: String,
    pub rest_api_port: u16,
    pub max_api_requests_per_minute: u32,
    pub rest_api_auth_enabled: bool,
    pub rest_api_username: Option<String>,
    pub rest_api_password: Option<String>,
    pub jwt_auth_enabled: bool,
    pub jwt_secret_key: Option<String>,
    pub jwt_expiration_minutes: u32,
    pub cors_enabled: bool,
    pub cors_allowed_origins: Vec<String>,
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
        dotenv().ok();

        let mqtt_root_topic = env::var("MQTT_ROOT_TOPIC").unwrap_or_else(|_| "image_uploader".to_string());

        let config = Self {
            // MQTT Configuration
            mqtt_host: env::var("MQTT_HOST").map_err(|_| ConfigError::MissingOrInvalid("MQTT_HOST".to_string()))?,
            mqtt_port: env::var("MQTT_PORT")
                .map_err(|_| ConfigError::MissingOrInvalid("MQTT_PORT".to_string()))?
                .parse::<u16>()
                .map_err(|_| ConfigError::ParsingError("MQTT_PORT must be a valid number".to_string()))?,
            mqtt_username: env::var("MQTT_USERNAME").unwrap_or_default(),
            mqtt_password: env::var("MQTT_PASSWORD").unwrap_or_default(),
            mqtt_ssl_enabled: env::var("MQTT_SSL_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse::<bool>()
                .map_err(|_| ConfigError::ParsingError("MQTT_SSL_ENABLED must be a boolean".to_string()))?,
            mqtt_ssl_cert_path: env::var("MQTT_SSL_CERT_PATH").ok(),
            mqtt_max_retries: env::var("MQTT_MAX_RETRIES")
                .unwrap_or_else(|_| "-1".to_string())
                .parse::<i32>()
                .map_err(|_| ConfigError::ParsingError("MQTT_MAX_RETRIES must be an integer".to_string()))?,
            mqtt_retry_interval_ms: env::var("MQTT_RETRY_INTERVAL_MS")
                .unwrap_or_else(|_| "5000".to_string())
                .parse::<u64>()
                .map_err(|_| ConfigError::ParsingError("MQTT_RETRY_INTERVAL_MS must be a valid number".to_string()))?,

            // MQTT Topics
            log_topic: format!("{}/logs", mqtt_root_topic),
            status_topic: format!("{}/status", mqtt_root_topic),
            command_topic: format!("{}/commands", mqtt_root_topic),
            progress_topic: format!("{}/progress", mqtt_root_topic),
            analytics_topic: format!("{}/analytics", mqtt_root_topic),

            // REST API Configuration
            rest_api_host: env::var("REST_API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            rest_api_port: env::var("REST_API_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse::<u16>()
                .map_err(|_| ConfigError::ParsingError("REST_API_PORT must be a valid number".to_string()))?,
            max_api_requests_per_minute: env::var("MAX_API_REQUESTS_PER_MINUTE")
                .unwrap_or_else(|_| "100".to_string())
                .parse::<u32>()
                .map_err(|_| ConfigError::ParsingError("MAX_API_REQUESTS_PER_MINUTE must be a valid number".to_string()))?,
            rest_api_auth_enabled: env::var("REST_API_AUTH_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse::<bool>()
                .map_err(|_| ConfigError::ParsingError("REST_API_AUTH_ENABLED must be a boolean".to_string()))?,
            rest_api_username: env::var("REST_API_USERNAME").ok(),
            rest_api_password: env::var("REST_API_PASSWORD").ok(),
            jwt_auth_enabled: env::var("JWT_AUTH_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse::<bool>()
                .map_err(|_| ConfigError::ParsingError("JWT_AUTH_ENABLED must be a boolean".to_string()))?,
            jwt_secret_key: env::var("JWT_SECRET_KEY").ok(),
            jwt_expiration_minutes: env::var("JWT_EXPIRATION_MINUTES")
                .unwrap_or_else(|_| "60".to_string())
                .parse::<u32>()
                .map_err(|_| ConfigError::ParsingError("JWT_EXPIRATION_MINUTES must be a valid number".to_string()))?,
            cors_enabled: env::var("CORS_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse::<bool>()
                .map_err(|_| ConfigError::ParsingError("CORS_ENABLED must be a boolean".to_string()))?,
            cors_allowed_origins: env::var("CORS_ALLOWED_ORIGINS")
                .unwrap_or_else(|_| "http://localhost".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
        };

        config.validate_timeouts()?;
        Ok(config)
    }
}

