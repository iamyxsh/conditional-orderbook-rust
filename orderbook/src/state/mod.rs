use crate::repositories::OrderRepository;
use actix_web::web::Data;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub orders: Arc<dyn OrderRepository>,
}

impl AppState {
    pub fn new<R: OrderRepository + 'static>(orders: R) -> Data<Self> {
        Data::new(Self {
            orders: Arc::new(orders),
        })
    }
}
