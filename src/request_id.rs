use axum::{
    extract::{FromRequestParts, State},
    http::{request::Parts, HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use http::Request;
use std::fmt::{self, Display, Formatter};
use uuid::Uuid;

/// The header name for the request ID.
pub const X_REQUEST_ID_HEADER: &str = "x-request-id";

/// A wrapper around a request ID (UUID).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct XRequestId(Uuid);

impl XRequestId {
    /// Generates a new random request ID.
    pub fn new_random() -> Self {
        Self(Uuid::new_v4())
    }

    /// Tries to parse a request ID from a string.
    pub fn from_str(s: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(s).map(Self)
    }

    /// Returns the underlying UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Display for XRequestId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for XRequestId
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;
        if let Some(header_value) = headers.get(X_REQUEST_ID_HEADER) {
            let header_str = header_value
                .to_str()
                .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid x-request-id header".to_string()))?;
            
            match XRequestId::from_str(header_str) {
                Ok(id) => {
                    tracing::debug!("Extracted existing x-request-id: {}", id);
                    Ok(id)
                },
                Err(_) => {
                    // If the provided ID is invalid, generate a new one and log the issue.
                    let new_id = XRequestId::new_random();
                    tracing::warn!(
                        "Invalid x-request-id header received: '{}'. Generating new ID: {}",
                        header_str,
                        new_id
                    );
                    Ok(new_id)
                }
            }
        } else {
            let new_id = XRequestId::new_random();
            tracing::debug!("Generated new x-request-id: {}", new_id);
            Ok(new_id)
        }
    }
}

/// Middleware to ensure every request has an `x-request-id`.
///
/// If an `x-request-id` header is present and valid, it is used.
/// Otherwise, a new UUID v4 is generated. The ID is then added to the response
/// headers and recorded in the current `tracing` span.
pub async fn request_id_middleware<B>(
    request_id: XRequestId,
    request: Request<B>,
    next: Next<B>,
) -> Response {
    let span = tracing::Span::current();
    span.record("request_id", request_id.to_string().as_str());

    let mut response = next.run(request).await;

    response.headers_mut().insert(
        X_REQUEST_ID_HEADER,
        HeaderValue::from_str(&request_id.to_string())
            .expect("UUID should always be a valid header value"),
    );

    response
}