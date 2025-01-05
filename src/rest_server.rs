use rocket::serde::{json::Json, Deserialize, Serialize};
use rocket::http::Status;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::{get, post, routes, State};
use rocket::figment::Figment;
use rusqlite::Result;
use crate::config::Config;
use crate::db::DatabaseService;

/// API Request payload
#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
struct ApiRequest {
    action: String,
    data: Option<String>,
}

/// API Response
#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct ApiResponse {
    status: String,
    message: String,
}

/// Struct for last value response
#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct LastValueResponse {
    topic: String,
    value: String,
    timestamp: String,
}

/// Struct for multiple values response
#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct LastValuesResponse {
    topic: String,
    values: Vec<(String, String)>, // Vec<(value, timestamp)>
}

/// CORS Fairing with Config support
pub struct Cors {
    allowed_origins: Vec<String>,
}

impl Cors {
    pub fn new(config: &Config) -> Self {
        Self {
            allowed_origins: config.cors_allowed_origins.clone(),
        }
    }
}

#[rocket::async_trait]
impl Fairing for Cors {
    fn info(&self) -> Info {
        Info {
            name: "CORS",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, req: &'r rocket::Request<'_>, res: &mut rocket::Response<'r>) {
        if let Some(origin) = req.headers().get_one("Origin") {
            if self.allowed_origins.contains(&origin.to_string())
                || self.allowed_origins.contains(&"*".to_string())
            {
                res.set_header(rocket::http::Header::new("Access-Control-Allow-Origin", origin));
            }
        }
        res.set_header(rocket::http::Header::new(
            "Access-Control-Allow-Methods",
            "GET, POST",
        ));
        res.set_header(rocket::http::Header::new(
            "Access-Control-Allow-Headers",
            "Content-Type",
        ));
    }
}

/// Get the last value of a topic
#[get("/topics/<topic>/last")]
fn last_value(
    topic: String,
    db: &State<DatabaseService>,
) -> Result<Json<LastValueResponse>, Status> {
    match db.get_last_value(&topic) {
        Ok(Some((value, timestamp))) => Ok(Json(LastValueResponse {
            topic,
            value,
            timestamp,
        })),
        Ok(None) => Err(Status::NotFound),
        Err(_) => Err(Status::InternalServerError),
    }
}

/// Get the last `n` values of a topic
#[get("/topics/<topic>/values?<limit>")]
fn last_values(
    topic: String,
    limit: Option<usize>,
    db: &State<DatabaseService>,
) -> Result<Json<LastValuesResponse>, Status> {
    let limit = limit.unwrap_or(10); // Default limit is 10
    match db.get_last_values(&topic, limit) {
        Ok(values) => Ok(Json(LastValuesResponse { topic, values })),
        Err(_) => Err(Status::InternalServerError),
    }
}

/// Root handler
#[get("/")]
fn root_handler(config: &State<Config>) -> Json<ApiResponse> {
    Json(ApiResponse {
        status: "success".to_string(),
        message: format!(
            "Welcome to the REST API running on {}:{}!",
            config.rest_api_host, config.rest_api_port
        ),
    })
}

/// Action handler
#[post("/action", data = "<payload>")]
fn action_handler(payload: Json<ApiRequest>, config: &State<Config>) -> Result<Json<ApiResponse>, Status> {
    if config.rest_api_auth_enabled {
        return Ok(Json(ApiResponse {
            status: "error".to_string(),
            message: "Authentication required but not implemented.".to_string(),
        }));
    }
    match payload.action.as_str() {
        "ping" => Ok(Json(ApiResponse {
            status: "success".to_string(),
            message: "pong".to_string(),
        })),
        _ => Ok(Json(ApiResponse {
            status: "error".to_string(),
            message: "Unknown action".to_string(),
        })),
    }
}

/// Run the Rocket server with the provided DatabaseService and Config
pub async fn run_rest_server(db_service: DatabaseService, config: Config) {
    // Clone `config` where necessary to avoid ownership issues
    let figment = Figment::from(rocket::Config::default())
        .merge(("address", config.rest_api_host.clone()))
        .merge(("port", config.rest_api_port));

    rocket::custom(figment)
        .manage(config.clone()) // Clone `Config` for Rocket's managed state
        .manage(db_service)
        .mount(
            "/",
            routes![root_handler, action_handler, last_value, last_values],
        )
        .attach(Cors::new(&config)) // Use `config` reference
        .launch()
        .await
        .unwrap();
}
