#[macro_use]
extern crate rocket;

use rocket::serde::{json::Json, Deserialize, Serialize};
use rocket::http::Status;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::{get, launch, post, routes, Request};

use std::sync::Arc;

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

    async fn on_response<'r>(&self, _req: &'r Request<'_>, res: &mut rocket::Response<'r>) {
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

/// Run the Rocket server
#[launch]
fn rocket() -> _ {
    rocket::build()
        .mount("/", routes![root_handler, action_handler])
        .attach(Cors)
}
