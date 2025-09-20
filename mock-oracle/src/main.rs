use std::{collections::HashMap, time::Duration};

use actix::prelude::*;
use actix::Actor;
use actix::AsyncContext;
use actix_web::get;
use actix_web::web;
use actix_web::App;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use actix_web::HttpServer;
use actix_web_actors::ws;
use dotenvy::dotenv;
use rand::Rng;
use serde::Deserialize;
use serde::Serialize;
use tracing_subscriber::fmt::SubscriberBuilder;
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
struct AppState {
    pairs: Vec<String>,
    interval: Duration,
    bands: HashMap<String, PriceBand>,
}

struct PriceWs {
    pairs: Vec<String>,
    interval: Duration,
    baselines: HashMap<String, f64>,
    bands: HashMap<String, PriceBand>,
}

impl PriceWs {
    fn new(pairs: Vec<String>, interval: Duration, bands: HashMap<String, PriceBand>) -> Self {
        let baselines = pairs
            .iter()
            .map(|p| {
                let seed = if let Some(b) = bands.get(p) {
                    seed_price_in_band(*b)
                } else {
                    seed_price(p)
                };
                (p.clone(), seed)
            })
            .collect::<HashMap<_, _>>();

        Self {
            pairs,
            interval,
            baselines,
            bands,
        }
    }
}

impl Actor for PriceWs {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let interval = self.interval;
        ctx.run_interval(interval, |actor, ctx| {
            for pair in &actor.pairs {
                let prev = *actor.baselines.get(pair).unwrap_or(&100.0);

                let next = if let Some(b) = actor.bands.get(pair) {
                    step_price_in_band(prev, *b)
                } else {
                    step_price(prev)
                };

                actor.baselines.insert(pair.clone(), next);

                let tick = Tick {
                    pair: pair.clone(),
                    price: next,
                    ts_ms: now_ms(),
                };
                if let Ok(s) = serde_json::to_string(&tick) {
                    ctx.text(s);
                }
            }
        });
    }
}

#[derive(Debug, Serialize)]
struct Tick {
    pair: String,
    price: f64,
    ts_ms: i64,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    SubscriberBuilder::default()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:9001".into());
    let pairs_env = std::env::var("PAIRS").unwrap_or_else(|_| "BTC/USDT,ETH/USDT,SOL/USDT".into());
    let pairs: Vec<String> = pairs_env
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let interval_ms: u64 = std::env::var("INTERVAL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);

    let mut bands: HashMap<String, PriceBand> = HashMap::new();
    bands.insert(
        "ETH/USDT".into(),
        PriceBand {
            min: 3500.0,
            max: 3501.0,
        },
    );
    bands.insert(
        "BTC/USDT".into(),
        PriceBand {
            min: 100_000.0,
            max: 110_000.0,
        },
    );
    bands.insert(
        "SOL/USDT".into(),
        PriceBand {
            min: 200.0,
            max: 201.0,
        },
    );

    let state = AppState {
        pairs: pairs.clone(),
        interval: Duration::from_millis(interval_ms),
        bands,
    };

    tracing::info!("price-oracle-ws listening on {}", bind);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .service(ws_endpoint)
    })
    .bind(bind)?
    .run()
    .await
}

#[derive(Debug, Clone, Deserialize)]
struct WsQuery {
    pair: Option<String>,
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for PriceWs {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(bytes)) => ctx.pong(&bytes),
            Ok(ws::Message::Close(reason)) => ctx.close(reason),
            Ok(ws::Message::Text(_)) | Ok(ws::Message::Binary(_)) => {}
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
struct PriceBand {
    min: f64,
    max: f64,
}

#[get("/ws")]
async fn ws_endpoint(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
    q: web::Query<WsQuery>,
) -> Result<HttpResponse, actix_web::Error> {
    let pairs = match &q.pair {
        Some(p) => vec![p.clone()],
        None => state.pairs.clone(),
    };

    let mut selected_bands = HashMap::new();
    for p in &pairs {
        if let Some(b) = state.bands.get(p) {
            selected_bands.insert(p.clone(), *b);
        }
    }

    let actor = PriceWs::new(pairs, state.interval, selected_bands);
    ws::start(actor, &req, stream)
}

fn seed_price(pair: &str) -> f64 {
    let h = fxhash(pair) as f64;
    50.0 + (h % 500.0)
}

fn fxhash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn step_price_in_band(prev: f64, b: PriceBand) -> f64 {
    let mut rng = rand::thread_rng();
    let noise: f64 = rng.gen_range(-0.0005..0.0005);
    let drift_towards_mid = ((b.min + b.max) / 2.0 - prev) * 0.001;
    let next = prev * (1.0 + noise) + drift_towards_mid;
    next.clamp(b.min, b.max)
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn seed_price_in_band(b: PriceBand) -> f64 {
    (b.min + b.max) / 2.0
}

fn step_price(prev: f64) -> f64 {
    let mut rng = rand::thread_rng();
    let drift = 0.0002;
    let noise: f64 = rng.gen_range(-0.003..0.003);
    let next = prev * (1.0 + drift + noise);
    next.clamp(0.01, 1_000_000.0)
}
