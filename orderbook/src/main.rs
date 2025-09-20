use actix_web::{middleware::Logger, App, HttpServer};
use dotenvy::dotenv;
use tracing_subscriber::{fmt::SubscriberBuilder, EnvFilter};

use crate::repositories::in_memory::InMemoryOrderRepository;

pub mod entities;
pub mod errors;
pub mod handlers;
pub mod repositories;
pub mod routes;
pub mod state;
pub mod utils;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();

    SubscriberBuilder::default()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let state = state::AppState::new(InMemoryOrderRepository::default());

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(state.clone())
            .configure(routes::config)
    })
    .bind(std::env::var("SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into()))?
    .run()
    .await
}
