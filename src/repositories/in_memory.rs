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
