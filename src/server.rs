use axum::{
    extract::Query,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{date, models::Asteroid, service};

#[derive(Debug, Deserialize)]
struct DateQuery {
    date: String,
}

#[derive(Debug, Serialize)]
struct AsteroidResponse {
    date: String,
    source: String,
    asteroids: Vec<Asteroid>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ErrorResponse {
    error: String,
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/asteroids", get(asteroids))
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../web/index.html"))
}

async fn asteroids(Query(query): Query<DateQuery>) -> Result<Json<AsteroidResponse>, ApiError> {
    let selected_date = date::parse_date(&query.date)
        .map_err(|error| ApiError(StatusCode::BAD_REQUEST, error.to_string()))?;
    let date_label = selected_date.format("%Y-%m-%d").to_string();
    let loaded = tokio::task::spawn_blocking(move || service::load_data(selected_date))
        .await
        .map_err(|error| ApiError(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map_err(|error| ApiError(StatusCode::BAD_GATEWAY, format!("{error:#}")))?;

    Ok(Json(AsteroidResponse {
        date: date_label,
        source: loaded.source,
        asteroids: loaded.asteroids,
    }))
}

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(ErrorResponse { error: self.1 })).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn serves_the_web_page() {
        let response = router()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("NASA Asteroid Tracker"));
        assert!(html.contains("/api/asteroids"));
    }

    #[tokio::test]
    async fn rejects_invalid_dates_without_calling_nasa() {
        for date in ["", "2025-02-31", "2025-13-01", "wrong"] {
            let response = router()
                .oneshot(
                    Request::builder()
                        .uri(format!("/api/asteroids?date={date}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{date}");
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let error: ErrorResponse = serde_json::from_slice(&body).unwrap();
            assert!(!error.error.is_empty());
        }
    }
}
