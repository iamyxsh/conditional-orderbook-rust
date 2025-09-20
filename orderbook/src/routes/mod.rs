use crate::handlers;
use actix_web::web::{self, ServiceConfig};

pub fn config(cfg: &mut ServiceConfig) {
    cfg.service(web::scope("/health").route("", web::get().to(handlers::health::ping)))
        .service(
            web::scope("/orders")
                .route("", web::post().to(handlers::orders::create_order))
                .route("", web::get().to(handlers::orders::list_orders))
                .route("/{id}", web::get().to(handlers::orders::get_order))
                .route(
                    "/{id}/status",
                    web::put().to(handlers::orders::update_status),
                )
                .route("/{id}", web::delete().to(handlers::orders::delete_order)),
        );
}
