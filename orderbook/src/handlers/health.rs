use actix_web::{HttpResponse, Responder};

pub async fn ping() -> impl Responder {
    HttpResponse::Ok().body("pong")
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{body::to_bytes, test, web, App};

    #[actix_web::test]
    async fn ping_returns_pong() {
        let app = test::init_service(App::new().route("/ping", web::get().to(ping))).await;
        let req = test::TestRequest::get().uri("/ping").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body = to_bytes(resp.into_body()).await.unwrap();
        assert_eq!(&body[..], b"pong");
    }
}
