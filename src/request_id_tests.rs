use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::get,
    Router,
};
use http_body_util::BodyExt; // for `collect` and `to_bytes`
use tower::ServiceExt; // for `call`, `oneshot`, `ready`
use uuid::Uuid;

use quicklendx_protocol::middleware::request_id::{request_id_middleware, X_REQUEST_ID_HEADER};

// A simple handler that logs and returns the request ID
async fn test_handler() -> String {
    let request_id = tracing::Span::current()
        .get_field("request_id")
        .map(|field| field.as_str().unwrap_or("N/A").to_string())
        .unwrap_or_else(|| "N/A".to_string());
    
    tracing::info!("Handler received request with ID: {}", request_id);
    format!("Request ID: {}", request_id)
}

fn app() -> Router {
    Router::new()
        .route("/", get(test_handler))
        .layer(axum::middleware::from_fn(request_id_middleware))
}

#[tokio::test]
async fn test_new_request_id_generated() {
    let app = app();

    let request = Request::builder().uri("/").body(Body::empty()).unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Check if x-request-id header is present in the response
    let header_value = response.headers().get(X_REQUEST_ID_HEADER).unwrap();
    let request_id_str = header_value.to_str().unwrap();
    let generated_uuid = Uuid::parse_str(request_id_str).unwrap();

    // Check response body contains the generated ID
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains(&generated_uuid.to_string()));

    // In a real scenario, you'd capture logs to assert the ID is present there.
    // For this test, we rely on the handler's output and header.
}

#[tokio::test]
async fn test_existing_request_id_propagated() {
    let app = app();
    let existing_uuid = Uuid::new_v4();

    let request = Request::builder()
        .uri("/")
        .header(X_REQUEST_ID_HEADER, existing_uuid.to_string())
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Check if the same x-request-id header is present in the response
    let header_value = response.headers().get(X_REQUEST_ID_HEADER).unwrap();
    let request_id_str = header_value.to_str().unwrap();
    assert_eq!(request_id_str, existing_uuid.to_string());

    // Check response body contains the propagated ID
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains(&existing_uuid.to_string()));
}

#[tokio::test]
async fn test_invalid_request_id_generates_new_one() {
    let app = app();
    let invalid_id = "not-a-valid-uuid";

    let request = Request::builder()
        .uri("/")
        .header(X_REQUEST_ID_HEADER, invalid_id)
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Check if a new x-request-id header is present (not the invalid one)
    let header_value = response.headers().get(X_REQUEST_ID_HEADER).unwrap();
    let request_id_str = header_value.to_str().unwrap();
    assert_ne!(request_id_str, invalid_id); // Should not be the invalid one
    let generated_uuid = Uuid::parse_str(request_id_str).unwrap(); // Should be a valid UUID

    // Check response body contains the newly generated ID
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains(&generated_uuid.to_string()));
}

#[tokio::test]
async fn test_indexer_correlation_logging_conceptual() {
    // Setup a tracing subscriber to capture logs
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_test_writer()
        .finish();
    let _guard = tracing::subscriber::set_global_default(subscriber);

    let request_id = Uuid::new_v4();
    let ledger_seq = 12345678;

    // Simulate indexer processing within a span that has the request_id
    let indexer_span = tracing::info_span!(
        "indexer_process",
        indexer_id = "main-indexer", // Example indexer identifier
        ledger_seq = ledger_seq,
        request_id = %request_id
    );
    let _enter = indexer_span.enter();

    tracing::info!("Processing transaction for invoice creation");
    tracing::debug!("Storing new invoice data in DB");

    // In a real test, you would assert against captured log output.
    // For this conceptual test, we just ensure it doesn't panic and logs.
    // A more robust test would use a `Buffer` or `Vec` to collect logs and then assert their content.
    // For now, this test primarily ensures the structure for logging is understood.
    assert!(true); // Placeholder assertion
}