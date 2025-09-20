use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::entities::order::{NewOrder, Order, OrderSide, OrderStatus};
use crate::errors::ApiError;
use crate::repositories::ListOrdersQuery;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateOrderPayload {
    pub pair: String,
    pub side: OrderSide,
    pub price: f64,
    pub quantity: f64,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub pair: Option<String>,
    pub status: Option<OrderStatus>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStatusPayload {
    pub status: OrderStatus,
}

#[derive(Debug, Serialize)]
struct OrderResponse(Order);

pub async fn create_order(
    state: web::Data<AppState>,
    payload: web::Json<CreateOrderPayload>,
) -> Result<HttpResponse, ApiError> {
    let new = NewOrder {
        pair: payload.pair.clone(),
        side: payload.side.clone(),
        price: payload.price,
        quantity: payload.quantity,
    };
    let created = state
        .orders
        .create(new)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(HttpResponse::Created().json(OrderResponse(created)))
}

pub async fn list_orders(
    state: web::Data<AppState>,
    q: web::Query<ListQuery>,
) -> Result<HttpResponse, ApiError> {
    let items = state
        .orders
        .list(ListOrdersQuery {
            pair: q.pair.clone(),
            status: q.status.clone(),
            limit: q.limit,
            offset: q.offset,
        })
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(HttpResponse::Ok().json(items))
}

pub async fn get_order(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();
    let order = state
        .orders
        .get_by_id(&id)
        .await
        .map_err(|_| ApiError::NotFound)?;
    Ok(HttpResponse::Ok().json(OrderResponse(order)))
}

pub async fn update_status(
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<UpdateStatusPayload>,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();
    let updated = state
        .orders
        .set_status(&id, payload.status.clone())
        .await
        .map_err(|_| ApiError::NotFound)?;
    Ok(HttpResponse::Ok().json(OrderResponse(updated)))
}

pub async fn delete_order(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let id = path.into_inner();
    state
        .orders
        .delete(&id)
        .await
        .map_err(|_| ApiError::NotFound)?;
    Ok(HttpResponse::NoContent().finish())
}
