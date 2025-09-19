use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::utils::now_ms;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    New,
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub pair: String,
    pub side: OrderSide,
    pub price: f64,
    pub quantity: f64,
    pub status: OrderStatus,
    pub created: i64,
    pub updated: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewOrder {
    pub pair: String,
    pub side: OrderSide,
    pub price: f64,
    pub quantity: f64,
}

impl Order {
    pub fn new(pair: String, side: OrderSide, price: f64, quantity: f64) -> Self {
        let now = now_ms();
        Self {
            id: Uuid::new_v4().to_string(),
            pair,
            side,
            price,
            quantity,
            status: OrderStatus::New,
            created: now,
            updated: now,
        }
    }
}
