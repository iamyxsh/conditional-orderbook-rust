use crate::entities::order::{NewOrder, Order, OrderStatus};
use crate::repositories::{ListOrdersQuery, OrderRepository};
use crate::utils::now_ms;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Default)]
pub struct InMemoryOrderRepository {
    inner: Arc<RwLock<HashMap<String, Order>>>,
}

#[async_trait]
impl OrderRepository for InMemoryOrderRepository {
    async fn create(&self, new: NewOrder) -> Result<Order, String> {
        let mut map = self.inner.write().await;
        let order = Order::new(new.pair, new.side, new.price, new.quantity);
        map.insert(order.id.clone(), order.clone());
        Ok(order)
    }

    async fn get_by_id(&self, id: &str) -> Result<Order, String> {
        let map = self.inner.read().await;
        map.get(id).cloned().ok_or_else(|| "not found".into())
    }

    async fn list(&self, q: ListOrdersQuery) -> Result<Vec<Order>, String> {
        let map = self.inner.read().await;
        let mut items: Vec<Order> = map.values().cloned().collect();

        if let Some(pair) = q.pair {
            items.retain(|o| o.pair == pair);
        }
        if let Some(status) = q.status {
            items.retain(|o| o.status == status);
        }

        let start = q.offset.unwrap_or(0).max(0) as usize;
        let end = q
            .limit
            .filter(|&l| l > 0)
            .map(|l| start + l as usize)
            .unwrap_or(items.len());

        let end = end.min(items.len());
        if start >= items.len() {
            return Ok(vec![]);
        }

        Ok(items[start..end].to_vec())
    }

    async fn set_status(&self, id: &str, status: OrderStatus) -> Result<Order, String> {
        let mut map = self.inner.write().await;
        let o = map.get_mut(id).ok_or_else(|| "not found")?;
        o.status = status;
        o.updated = now_ms();
        Ok(o.clone())
    }

    async fn delete(&self, id: &str) -> Result<(), String> {
        let mut map = self.inner.write().await;
        map.remove(id).map(|_| ()).ok_or_else(|| "not found".into())
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use super::*;
    use crate::entities::order::{Order, OrderSide};

    fn sample_order(id: &str, pair: &str) -> Order {
        Order {
            id: id.to_string(),
            pair: pair.to_string(),
            side: OrderSide::Buy,
            price: dec!(100.0),
            quantity: dec!(1.0),
            status: OrderStatus::New,
            created: 1_700_000_000_000,
            updated: 1_700_000_000_000,
        }
    }

    async fn seed(repo: &InMemoryOrderRepository, orders: &[Order]) {
        let mut w = repo.inner.write().await;
        for o in orders {
            w.insert(o.id.clone(), o.clone());
        }
    }

    #[tokio::test]
    async fn set_status_updates_status_and_timestamp() {
        let repo = InMemoryOrderRepository::default();
        let id = "abc";
        let mut o = sample_order(id, "BTC/USDT");
        o.status = OrderStatus::New;
        o.updated = 1_700_000_000_000;
        seed(&repo, &[o]).await;

        let before = {
            let r = repo.inner.read().await;
            r.get(id).unwrap().clone()
        };

        let updated = repo.set_status(id, OrderStatus::Cancelled).await.unwrap();
        assert_eq!(updated.id, id);
        assert_eq!(updated.status, OrderStatus::Cancelled);
        assert!(updated.updated >= before.updated);

        let after = {
            let r = repo.inner.read().await;
            r.get(id).unwrap().clone()
        };
        assert_eq!(after.status, OrderStatus::Cancelled);
        assert!(after.updated >= before.updated);
    }

    #[tokio::test]
    async fn delete_removes_order() {
        let repo = InMemoryOrderRepository::default();
        let id = "deadbeef";
        seed(&repo, &[sample_order(id, "ETH/USDT")]).await;

        repo.delete(id).await.unwrap();

        let r = repo.inner.read().await;
        assert!(!r.contains_key(id));
    }

    #[tokio::test]
    async fn delete_nonexistent_returns_err() {
        let repo = InMemoryOrderRepository::default();
        let err = repo.delete("nope").await.unwrap_err();
        assert!(!err.is_empty());
    }
}
