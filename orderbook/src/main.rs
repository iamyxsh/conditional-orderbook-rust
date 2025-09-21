use actix_web::{middleware::Logger, web, App, HttpServer};
use dotenvy::dotenv;
use tracing_subscriber::{fmt::SubscriberBuilder, EnvFilter};

use crate::engine::start_matchers;
use crate::oracle_service::{OracleCache, OracleWsClient};
use crate::repositories::in_memory::InMemoryOrderRepository;

pub mod engine;
pub mod entities;
pub mod errors;
pub mod handlers;
pub mod oracle_service;
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

    let cache = OracleCache::default();
    OracleWsClient::default().spawn(cache.clone());
    let cache_data = web::Data::new(cache.clone());

    let repo = InMemoryOrderRepository::default();
    let state = state::AppState::new(repo.clone());

    let assets = vec![
        "BTC/USDT".to_string(),
        "ETH/USDT".to_string(),
        "SOL/USDT".to_string(),
    ];

    start_matchers(
        assets,
        repo.clone(),
        cache.clone(),
        std::time::Duration::from_secs(1),
    );

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(state.clone())
            .app_data(cache_data.clone())
            .configure(routes::config)
    })
    .bind(std::env::var("SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into()))?
    .run()
    .await
}
