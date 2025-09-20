use futures_util::StreamExt;
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{sync::RwLock, time::sleep};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(Debug, Clone, Deserialize)]
pub struct Tick {
    pub pair: String,
    pub price: f64,
    pub ts_ms: i64,
}

#[derive(Clone, Default)]
pub struct OracleCache {
    inner: Arc<RwLock<HashMap<String, Tick>>>,
}

impl OracleCache {
    pub async fn set(&self, t: Tick) {
        let mut w = self.inner.write().await;
        w.insert(t.pair.clone(), t);
    }

    pub async fn get_price(&self, pair: &str) -> Option<(f64, i64)> {
        let r = self.inner.read().await;
        r.get(pair).map(|t| (t.price, t.ts_ms))
    }

    pub async fn pairs(&self) -> Vec<String> {
        let r = self.inner.read().await;
        r.keys().cloned().collect()
    }
}

pub struct OracleWsClient {
    pub endpoint: String,
    pub pair: Option<String>,
    pub reconnect_backoff: Duration,
}

impl Default for OracleWsClient {
    fn default() -> Self {
        Self {
            endpoint: "ws://127.0.0.1:9001/ws".into(),
            pair: None,
            reconnect_backoff: Duration::from_secs(2),
        }
    }
}

impl OracleWsClient {
    pub fn spawn(self, cache: OracleCache) {
        tokio::spawn(async move {
            let mut backoff = self.reconnect_backoff;
            loop {
                let url = build_url(&self.endpoint, self.pair.as_deref());
                tracing::info!("oracle-ws: connecting to {}", url);

                match connect_async(&url).await {
                    Ok((ws_stream, _resp)) => {
                        tracing::info!("oracle-ws: connected");
                        backoff = self.reconnect_backoff;

                        let (_, mut read) = ws_stream.split();
                        while let Some(msg) = read.next().await {
                            match msg {
                                Ok(Message::Text(txt)) => {
                                    match serde_json::from_str::<Tick>(&txt) {
                                        Ok(tick) => cache.set(tick).await,
                                        Err(e) => {
                                            tracing::warn!("oracle-ws: bad json: {e}; raw={txt}")
                                        }
                                    }
                                }
                                Ok(Message::Binary(_bin)) => {}
                                Ok(Message::Ping(p)) => {
                                    let _ = p;
                                }
                                Ok(Message::Close(c)) => {
                                    tracing::warn!("oracle-ws: server closed: {:?}", c);
                                    break;
                                }
                                Err(e) => {
                                    tracing::warn!("oracle-ws: read error: {e}");
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("oracle-ws: connect failed: {e}");
                    }
                }

                tracing::info!("oracle-ws: reconnecting in {:?}", backoff);
                sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(30));
                tokio::task::yield_now().await;
            }
        });
    }
}

fn build_url(base: &str, pair: Option<&str>) -> String {
    if let Some(p) = pair {
        let mut u = url::Url::parse(base).expect("invalid ws endpoint");
        let mut q = u.query_pairs_mut();
        q.append_pair("pair", p);
        drop(q);
        u.to_string()
    } else {
        base.to_string()
    }
}
