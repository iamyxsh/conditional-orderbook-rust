mod in_memory;

use async_trait::async_trait;

use crate::{
    entities::order::{NewOrder, Order, OrderStatus},

};

#[derive(Debug, Clone, Default)]
pub struct ListOrdersQuery {
    pub pair: Option<String>,
    pub status: Option<OrderStatus>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[async_trait]
pub trait OrderRepository: Send + Sync {
    async fn create(&self, new: NewOrder) -> Result<Order, String>;
    async fn get_by_id(&self, id: &str) -> Result<Order, String>;
    async fn list(&self, q: ListOrdersQuery) -> Result<Vec<Order>, String>;
    async fn set_status(&self, id: &str, status: OrderStatus) -> Result<Order, String>;
    async fn delete(&self, id: &str) -> Result<(), String>;
}
