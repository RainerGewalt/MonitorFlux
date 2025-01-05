use rocket::serde::{json::Json, Deserialize, Serialize};
use rocket::http::Status;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::{get, post, routes, State};
use rusqlite::{Result};
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


/// CORS Fairing for Rocket
pub struct Cors;

#[rocket::async_trait]
impl Fairing for Cors {
    fn info(&self) -> Info {
        Info {
            name: "CORS",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, _req: &'r rocket::Request<'_>, res: &mut rocket::Response<'r>) {
        res.set_header(rocket::http::Header::new("Access-Control-Allow-Origin", "*"));
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
fn root_handler() -> Json<ApiResponse> {
    Json(ApiResponse {
        status: "success".to_string(),
        message: "Welcome to the REST API!".to_string(),
    })
}

/// Action handler
#[post("/action", data = "<payload>")]
fn action_handler(payload: Json<ApiRequest>) -> Result<Json<ApiResponse>, Status> {
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

/// Run the Rocket server with the provided DatabaseService
pub async fn run_rest_server(db_service: DatabaseService) {
    rocket::build()
        .manage(db_service)
        .mount(
            "/",
            routes![root_handler, action_handler, last_value, last_values],
        )
        .attach(Cors)
        .launch()
        .await
        .unwrap();
}
