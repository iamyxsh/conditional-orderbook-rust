use actix_web::test::{self, TestRequest};
use actix_web::{http::StatusCode, App};
use serde_json::json;

use conditional_orderbook::{
    entities::order::{Order, OrderStatus},
    repositories::in_memory::InMemoryOrderRepository,
    routes,
    state::AppState,
};

fn test_app() -> actix_web::App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    let state = AppState::new(InMemoryOrderRepository::default());
    App::new().app_data(state).configure(routes::config)
}

#[actix_web::test]
async fn health_ok() {
    let app = test::init_service(test_app()).await;

    let req = TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;

    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn orders_create_and_get() {
    let app = test::init_service(test_app()).await;

    let payload = json!({
        "pair": "BTC/USDT",
        "side": "buy",
        "price": 25000.5,
        "quantity": 0.1
    });
    let req = TestRequest::post()
        .uri("/orders")
        .set_json(&payload)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::CREATED);

    let created: Order = test::read_body_json(resp).await;
    assert_eq!(created.pair, "BTC/USDT");
    assert_eq!(created.price, 25000.5);
    assert_eq!(created.quantity, 0.1);
    assert_eq!(created.status, OrderStatus::New);
    assert!(!created.id.is_empty());

    let req = TestRequest::get()
        .uri(&format!("/orders/{}", created.id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let fetched: Order = test::read_body_json(resp).await;
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.pair, "BTC/USDT");
}

#[actix_web::test]
async fn orders_list_then_update_status_then_delete() {
    let app = test::init_service(test_app()).await;

    let payload = json!({
        "pair": "ETH/USDT",
        "side": "sell",
        "price": 3100.0,
        "quantity": 2.0
    });
    let req = TestRequest::post()
        .uri("/orders")
        .set_json(&payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let created: Order = test::read_body_json(resp).await;

    let req = TestRequest::get().uri("/orders").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let list: Vec<Order> = test::read_body_json(resp).await;
    assert_eq!(list.len(), 1);

    let req = TestRequest::put()
        .uri(&format!("/orders/{}/status", created.id))
        .set_json(&json!({ "status": "open" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let updated: Order = test::read_body_json(resp).await;
    assert_eq!(updated.status, OrderStatus::Open);

    let req = TestRequest::delete()
        .uri(&format!("/orders/{}", created.id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let req = TestRequest::get()
        .uri(&format!("/orders/{}", created.id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
