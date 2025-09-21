use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrderBook {
    pub pair: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn default_is_empty_pair() {
        let ob = OrderBook::default();
        assert_eq!(ob.pair, "");
    }

    #[test]
    fn roundtrip_serde() {
        let ob = OrderBook {
            pair: "BTC/USDT".into(),
        };
        let s = serde_json::to_string(&ob).unwrap();
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["pair"], "BTC/USDT");
        let back: OrderBook = serde_json::from_str(&s).unwrap();
        assert_eq!(back.pair, "BTC/USDT");
    }
}
