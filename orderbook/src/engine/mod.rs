use std::time::Duration;
use tokio::time::{interval, MissedTickBehavior};
use tracing::{debug, error, info, instrument};

use crate::entities::order::{Order, OrderSide, OrderStatus};
use crate::oracle_service::OracleCache;
use crate::repositories::{ListOrdersQuery, OrderRepository};

pub fn start_matchers<R: OrderRepository + Clone + 'static>(
    assets: Vec<String>,
    repo: R,
    oracle: OracleCache,
    tick_every: Duration,
) {
    for asset in assets {
        let repo_cloned = repo.clone();
        let oracle_cloned = oracle.clone();
        tokio::spawn(async move {
            run_worker(asset, repo_cloned, oracle_cloned, tick_every).await;
        });
    }
}

#[instrument(name = "matcher_worker", skip(repo, oracle), fields(%asset, tick_ms = %tick_every.as_millis()))]
async fn run_worker<R: OrderRepository>(
    asset: String,
    repo: R,
    oracle: OracleCache,
    tick_every: Duration,
) {
    let mut t = interval(tick_every);
    t.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut ticks: u64 = 0;

    loop {
        t.tick().await;
        ticks += 1;

        let Some((px, ts)) = oracle.get_price(&asset).await else {
            debug!(%asset, tick = ticks, "no oracle price yet; skipping this tick");
            continue;
        };

        let mut active: Vec<Order> = Vec::new();
        for status in [
            OrderStatus::New,
            OrderStatus::Open,
            OrderStatus::PartiallyFilled,
        ] {
            match repo
                .list(ListOrdersQuery {
                    pair: Some(asset.clone()),
                    status: Some(status.clone()),
                    limit: None,
                    offset: None,
                })
                .await
            {
                Ok(mut v) => active.append(&mut v),
                Err(e) => {
                    error!(%asset, ?status, err = %e, "failed to list orders");
                }
            }
        }

        info!(%asset, tick = ticks, oracle_px = px, oracle_ts = ts, active = active.len(), "tick");

        if active.is_empty() {
            debug!(%asset, tick = ticks, "no active orders");
            continue;
        }

        let mut matched = 0usize;
        let mut promoted = 0usize;

        for o in active {
            if crosses(&o, px) {
                match repo.set_status(&o.id, OrderStatus::Filled).await {
                    Ok(filled) => {
                        matched += 1;
                        log_exec(&filled, px, ts);
                    }
                    Err(e) => {
                        error!(%asset, order_id = %o.id, err = %e, "failed to set status=Filled");
                    }
                }
            } else if matches!(o.status, OrderStatus::New) {
                match repo.set_status(&o.id, OrderStatus::Open).await {
                    Ok(_) => {
                        promoted += 1;
                        debug!(%asset, order_id = %o.id, limit_px = o.price, oracle_px = px, "promoted NEW -> OPEN (not crossing)");
                    }
                    Err(e) => {
                        error!(%asset, order_id = %o.id, err = %e, "failed to promote NEW -> OPEN");
                    }
                }
            } else {
                debug!(%asset, order_id = %o.id, status = ?o.status, limit_px = o.price, oracle_px = px, "not crossing");
            }
        }

        info!(%asset, tick = ticks, matched, promoted, "tick summary");
    }
}

fn crosses(o: &Order, oracle_px: f64) -> bool {
    match o.side {
        OrderSide::Buy => o.price >= oracle_px,
        OrderSide::Sell => o.price <= oracle_px,
    }
}

fn log_exec(o: &Order, px: f64, ts_ms: i64) {
    info!(
        pair = %o.pair,
        side = ?o.side,
        order_id = %o.id,
        qty = o.quantity,
        limit_px = o.price,
        exec_px = px,
        oracle_ts = ts_ms,
        "EXECUTE"
    );
}
