use rust_decimal::Decimal;
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
    pub price: Decimal,
    pub quantity: Decimal,
    pub status: OrderStatus,
    pub created: i64,
    pub updated: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewOrder {
    pub pair: String,
    pub side: OrderSide,
    pub price: Decimal,
    pub quantity: Decimal,
}

impl Order {
    pub fn new(pair: String, side: OrderSide, price: Decimal, quantity: Decimal) -> Self {
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

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use super::*;

    fn is_plausible_ms(ts: i64) -> bool {
        (1_000_000_000_000..=4_000_000_000_000).contains(&ts)
    }

    #[test]
    fn order_new_populates_fields() {
        let o = Order::new("BTC/USDT".into(), OrderSide::Buy, dec!(100.5), dec!(2.0));
        assert_eq!(o.pair, "BTC/USDT");
        assert_eq!(o.side, OrderSide::Buy);
        assert_eq!(o.price, dec!(100.5));
        assert_eq!(o.quantity, dec!(2.0));
        assert_eq!(o.status, OrderStatus::New);
        assert!(!o.id.is_empty());
        assert!(
            is_plausible_ms(o.created),
            "created not plausible ms: {}",
            o.created
        );
        assert!(
            is_plausible_ms(o.updated),
            "updated not plausible ms: {}",
            o.updated
        );
        assert!(o.updated >= o.created);
    }

    #[test]
    fn order_side_serde_is_snake_case() {
        let s_buy = serde_json::to_string(&OrderSide::Buy).unwrap();
        let s_sell = serde_json::to_string(&OrderSide::Sell).unwrap();
        assert_eq!(s_buy, "\"buy\"");
        assert_eq!(s_sell, "\"sell\"");
        let back: OrderSide = serde_json::from_str(&s_buy).unwrap();
        assert_eq!(back, OrderSide::Buy);
    }

    #[test]
    fn order_status_serde_is_snake_case() {
        let s = serde_json::to_string(&OrderStatus::New).unwrap();
        assert_eq!(s, "\"new\"");
        let back: OrderStatus = serde_json::from_str(&s).unwrap();
        assert_eq!(back, OrderStatus::New);
    }
}
