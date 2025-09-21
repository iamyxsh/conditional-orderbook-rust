use rust_decimal::Decimal;
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

async fn collect_active_orders<R: OrderRepository>(asset: &str, repo: &R) -> Vec<Order> {
    let mut active: Vec<Order> = Vec::new();
    for status in [
        OrderStatus::New,
        OrderStatus::Open,
        OrderStatus::PartiallyFilled,
    ] {
        match repo
            .list(ListOrdersQuery {
                pair: Some(asset.to_string()),
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
    active
}

async fn process_active_orders<R: OrderRepository>(
    asset: &str,
    repo: &R,
    orders: Vec<Order>,
    px: Decimal,
    ts_ms: i64,
) -> (usize, usize) {
    let mut matched = 0usize;
    let mut promoted = 0usize;
    for o in orders {
        if crosses(&o, px) {
            match repo.set_status(&o.id, OrderStatus::Filled).await {
                Ok(filled) => {
                    matched += 1;
                    log_exec(&filled, px, ts_ms);
                }
                Err(e) => {
                    error!(%asset, order_id = %o.id, err = %e, "failed to set status=Filled");
                }
            }
        } else if matches!(o.status, OrderStatus::New) {
            match repo.set_status(&o.id, OrderStatus::Open).await {
                Ok(_) => {
                    promoted += 1;
                    debug!(%asset, order_id = %o.id, limit_px = o.price.to_string(), oracle_px = px.to_string(), "promoted NEW -> OPEN (not crossing)");
                }
                Err(e) => {
                    error!(%asset, order_id = %o.id, err = %e, "failed to promote NEW -> OPEN");
                }
            }
        } else {
            debug!(%asset, order_id = %o.id, status = ?o.status, limit_px = o.price.to_string(), oracle_px = px.to_string(), "not crossing");
        }
    }
    (matched, promoted)
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
        let active = collect_active_orders(&asset, &repo).await;
        info!(%asset, tick = ticks, oracle_px = px.to_string(), oracle_ts = ts, active = active.len(), "tick");
        if active.is_empty() {
            debug!(%asset, tick = ticks, "no active orders");
            continue;
        }
        let (matched, promoted) = process_active_orders(&asset, &repo, active, px, ts).await;
        info!(%asset, tick = ticks, matched, promoted, "tick summary");
    }
}

fn crosses(o: &Order, oracle_px: Decimal) -> bool {
    match o.side {
        OrderSide::Buy => o.price >= oracle_px,
        OrderSide::Sell => o.price <= oracle_px,
    }
}

fn log_exec(o: &Order, px: Decimal, ts_ms: i64) {
    info!(
        pair      = %o.pair,
        side      = ?o.side,
        order_id  = %o.id,
        qty       = %o.quantity,
        limit_px  = %o.price,
        exec_px   = %px,
        oracle_ts = ts_ms,
        "EXECUTE"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    use crate::entities::order::{NewOrder, Order, OrderSide, OrderStatus};
    use crate::repositories::{ListOrdersQuery, OrderRepository};
    use crate::utils::now_ms;

    #[derive(Clone, Default)]
    struct FakeRepo {
        inner: Arc<RwLock<HashMap<String, Order>>>,
        fail_list_on_status: Arc<RwLock<Option<OrderStatus>>>,
        fail_set_for_ids: Arc<RwLock<HashSet<String>>>,
    }

    impl FakeRepo {
        async fn set_fail_list_on(&self, s: Option<OrderStatus>) {
            *self.fail_list_on_status.write().await = s;
        }
        async fn fail_set_for(&self, id: &str) {
            self.fail_set_for_ids.write().await.insert(id.to_string());
        }
    }

    #[async_trait::async_trait]
    impl OrderRepository for FakeRepo {
        async fn list(&self, q: ListOrdersQuery) -> Result<Vec<Order>, String> {
            if let Some(status) = &q.status {
                if Some(status.clone()) == *self.fail_list_on_status.read().await {
                    return Err(format!("boom listing {:?}", status));
                }
            }
            let map = self.inner.read().await;
            let mut v: Vec<Order> = map
                .values()
                .cloned()
                .filter(|o| {
                    q.pair.as_ref().map_or(true, |p| &o.pair == p)
                        && q.status.as_ref().map_or(true, |s| &o.status == s)
                })
                .collect();
            if let Some(off) = q.offset {
                if off < v.len() as i64 {
                    v.drain(0..off as usize);
                } else {
                    v.clear();
                }
            }
            if let Some(lim) = q.limit {
                if v.len() > lim as usize {
                    v.truncate(lim as usize);
                }
            }
            Ok(v)
        }

        async fn set_status(&self, id: &str, to: OrderStatus) -> Result<Order, String> {
            if self.fail_set_for_ids.read().await.contains(id) {
                return Err("boom set_status".into());
            }
            let mut map = self.inner.write().await;
            let o = map.get_mut(id).ok_or_else(|| "not found".to_string())?;
            o.status = to;
            o.updated = now_ms();
            Ok(o.clone())
        }

        async fn create(&self, n: NewOrder) -> Result<Order, String> {
            let id = uuid::Uuid::new_v4().to_string();
            let o = Order {
                id,
                pair: n.pair,
                side: n.side,
                price: n.price,
                quantity: n.quantity,
                status: OrderStatus::New,
                created: now_ms(),
                updated: now_ms(),
            };
            let mut map = self.inner.write().await;
            map.insert(o.id.clone(), o.clone());
            Ok(o)
        }

        async fn get_by_id(&self, id: &str) -> Result<Order, String> {
            let map = self.inner.read().await;
            map.get(id).cloned().ok_or_else(|| "not found".to_string())
        }

        async fn delete(&self, id: &str) -> Result<(), String> {
            let mut map = self.inner.write().await;
            match map.remove(id) {
                Some(_) => Ok(()),
                None => Err("not found".to_string()),
            }
        }
    }

    fn mk_order(
        id: &str,
        pair: &str,
        side: OrderSide,
        price: &str,
        qty: &str,
        status: OrderStatus,
    ) -> Order {
        Order {
            id: id.to_string(),
            pair: pair.to_string(),
            side,
            price: Decimal::from_str_exact(price).unwrap(),
            quantity: Decimal::from_str_exact(qty).unwrap(),
            status,
            created: now_ms(),
            updated: now_ms(),
        }
    }

    async fn seed(repo: &FakeRepo, orders: Vec<Order>) {
        let mut w = repo.inner.write().await;
        for o in orders {
            w.insert(o.id.clone(), o);
        }
    }

    #[test]
    fn crosses_buy_triggers_when_oracle_at_or_below_limit() {
        let o = mk_order(
            "1",
            "BTC/USDT",
            OrderSide::Buy,
            "100.0",
            "1",
            OrderStatus::New,
        );
        assert!(super::crosses(&o, dec!(100.0)));
        assert!(super::crosses(&o, dec!(99.99)));
        assert!(!super::crosses(&o, dec!(100.01)));
    }

    #[test]
    fn crosses_sell_triggers_when_oracle_at_or_above_limit() {
        let o = mk_order(
            "1",
            "BTC/USDT",
            OrderSide::Sell,
            "100.0",
            "1",
            OrderStatus::New,
        );
        assert!(super::crosses(&o, dec!(100.0)));
        assert!(super::crosses(&o, dec!(100.01)));
        assert!(!super::crosses(&o, dec!(99.99)));
    }

    #[tokio::test]
    async fn collect_active_gathers_new_open_partial_for_asset() {
        let repo = FakeRepo::default();
        seed(
            &repo,
            vec![
                mk_order(
                    "n1",
                    "BTC/USDT",
                    OrderSide::Buy,
                    "100",
                    "1",
                    OrderStatus::New,
                ),
                mk_order(
                    "o1",
                    "BTC/USDT",
                    OrderSide::Buy,
                    "100",
                    "1",
                    OrderStatus::Open,
                ),
                mk_order(
                    "p1",
                    "BTC/USDT",
                    OrderSide::Buy,
                    "100",
                    "1",
                    OrderStatus::PartiallyFilled,
                ),
                mk_order(
                    "x1",
                    "BTC/USDT",
                    OrderSide::Buy,
                    "100",
                    "1",
                    OrderStatus::Filled,
                ),
                mk_order(
                    "e1",
                    "ETH/USDT",
                    OrderSide::Buy,
                    "100",
                    "1",
                    OrderStatus::New,
                ),
            ],
        )
        .await;
        let v = super::collect_active_orders("BTC/USDT", &repo).await;
        let ids: HashSet<_> = v.into_iter().map(|o| o.id).collect();
        assert_eq!(
            ids,
            HashSet::from(["n1".to_string(), "o1".to_string(), "p1".to_string(),])
        );
    }

    #[tokio::test]
    async fn collect_active_skips_on_list_error_but_keeps_others() {
        let repo = FakeRepo::default();
        seed(
            &repo,
            vec![
                mk_order(
                    "n1",
                    "BTC/USDT",
                    OrderSide::Buy,
                    "100",
                    "1",
                    OrderStatus::New,
                ),
                mk_order(
                    "o1",
                    "BTC/USDT",
                    OrderSide::Buy,
                    "100",
                    "1",
                    OrderStatus::Open,
                ),
                mk_order(
                    "p1",
                    "BTC/USDT",
                    OrderSide::Buy,
                    "100",
                    "1",
                    OrderStatus::PartiallyFilled,
                ),
            ],
        )
        .await;
        repo.set_fail_list_on(Some(OrderStatus::Open)).await;
        let v = super::collect_active_orders("BTC/USDT", &repo).await;
        let ids: HashSet<_> = v.into_iter().map(|o| o.id).collect();
        assert_eq!(ids, HashSet::from(["n1".to_string(), "p1".to_string()]));
    }

    #[tokio::test]
    async fn promotes_new_to_open_when_not_crossing() {
        let repo = FakeRepo::default();
        seed(
            &repo,
            vec![mk_order(
                "o1",
                "BTC/USDT",
                OrderSide::Buy,
                "100.0",
                "1",
                OrderStatus::New,
            )],
        )
        .await;
        let (matched, promoted) = super::process_active_orders(
            "BTC/USDT",
            &repo,
            vec![repo.get_by_id("o1").await.unwrap()],
            dec!(101.0),
            1_700_000_000_000,
        )
        .await;
        assert_eq!(matched, 0);
        assert_eq!(promoted, 1);
        assert_eq!(
            repo.get_by_id("o1").await.unwrap().status,
            OrderStatus::Open
        );
    }

    #[tokio::test]
    async fn executes_when_crossing_buy_for_all_statuses() {
        let repo = FakeRepo::default();
        seed(
            &repo,
            vec![
                mk_order(
                    "n",
                    "BTC/USDT",
                    OrderSide::Buy,
                    "100",
                    "1",
                    OrderStatus::New,
                ),
                mk_order(
                    "o",
                    "BTC/USDT",
                    OrderSide::Buy,
                    "100",
                    "1",
                    OrderStatus::Open,
                ),
                mk_order(
                    "p",
                    "BTC/USDT",
                    OrderSide::Buy,
                    "100",
                    "1",
                    OrderStatus::PartiallyFilled,
                ),
            ],
        )
        .await;
        let orders = vec![
            repo.get_by_id("n").await.unwrap(),
            repo.get_by_id("o").await.unwrap(),
            repo.get_by_id("p").await.unwrap(),
        ];
        let (matched, promoted) =
            super::process_active_orders("BTC/USDT", &repo, orders, dec!(100.0), 1_700_000_000_000)
                .await;
        assert_eq!(matched, 3);
        assert_eq!(promoted, 0);
        for id in ["n", "o", "p"] {
            assert_eq!(
                repo.get_by_id(id).await.unwrap().status,
                OrderStatus::Filled
            );
        }
    }

    #[tokio::test]
    async fn leaves_open_unchanged_when_not_crossing() {
        let repo = FakeRepo::default();
        seed(
            &repo,
            vec![mk_order(
                "o1",
                "BTC/USDT",
                OrderSide::Buy,
                "100",
                "1",
                OrderStatus::Open,
            )],
        )
        .await;
        let (matched, promoted) = super::process_active_orders(
            "BTC/USDT",
            &repo,
            vec![repo.get_by_id("o1").await.unwrap()],
            dec!(101.0),
            1_700_000_000_000,
        )
        .await;
        assert_eq!(matched, 0);
        assert_eq!(promoted, 0);
        assert_eq!(
            repo.get_by_id("o1").await.unwrap().status,
            OrderStatus::Open
        );
    }

    #[tokio::test]
    async fn respects_sell_crossing_direction() {
        let repo = FakeRepo::default();
        seed(
            &repo,
            vec![
                mk_order(
                    "s1",
                    "BTC/USDT",
                    OrderSide::Sell,
                    "100",
                    "1",
                    OrderStatus::New,
                ),
                mk_order(
                    "s2",
                    "BTC/USDT",
                    OrderSide::Sell,
                    "100",
                    "1",
                    OrderStatus::Open,
                ),
            ],
        )
        .await;
        let orders = vec![
            repo.get_by_id("s1").await.unwrap(),
            repo.get_by_id("s2").await.unwrap(),
        ];
        let (matched, promoted) =
            super::process_active_orders("BTC/USDT", &repo, orders, dec!(100.5), 1_700_000_000_000)
                .await;
        assert_eq!(matched, 2);
        assert_eq!(promoted, 0);
        for id in ["s1", "s2"] {
            assert_eq!(
                repo.get_by_id(id).await.unwrap().status,
                OrderStatus::Filled
            );
        }
    }

    #[tokio::test]
    async fn set_status_error_does_not_increment_counters() {
        let repo = FakeRepo::default();
        seed(
            &repo,
            vec![
                mk_order(
                    "ok",
                    "BTC/USDT",
                    OrderSide::Buy,
                    "100",
                    "1",
                    OrderStatus::Open,
                ),
                mk_order(
                    "bad",
                    "BTC/USDT",
                    OrderSide::Buy,
                    "100",
                    "1",
                    OrderStatus::Open,
                ),
            ],
        )
        .await;
        repo.fail_set_for("bad").await;
        let orders = vec![
            repo.get_by_id("ok").await.unwrap(),
            repo.get_by_id("bad").await.unwrap(),
        ];
        let (matched, promoted) =
            super::process_active_orders("BTC/USDT", &repo, orders, dec!(100.0), 1_700_000_000_000)
                .await;
        assert_eq!(matched, 1);
        assert_eq!(promoted, 0);
        assert_eq!(
            repo.get_by_id("ok").await.unwrap().status,
            OrderStatus::Filled
        );
        assert_eq!(
            repo.get_by_id("bad").await.unwrap().status,
            OrderStatus::Open
        );
    }

    #[tokio::test]
    async fn promotion_error_does_not_increment_promoted() {
        let repo = FakeRepo::default();
        seed(
            &repo,
            vec![mk_order(
                "n",
                "BTC/USDT",
                OrderSide::Buy,
                "100",
                "1",
                OrderStatus::New,
            )],
        )
        .await;
        repo.fail_set_for("n").await;
        let orders = vec![repo.get_by_id("n").await.unwrap()];
        let (matched, promoted) =
            super::process_active_orders("BTC/USDT", &repo, orders, dec!(101.0), 1_700_000_000_000)
                .await;
        assert_eq!(matched, 0);
        assert_eq!(promoted, 0);
        assert_eq!(repo.get_by_id("n").await.unwrap().status, OrderStatus::New);
    }
}
