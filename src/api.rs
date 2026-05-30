use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::{OpenApi, ToSchema};

use crate::client::{self, DepError};

pub const SERVICE: &str = "srvcs-outofrange";
pub const CONCERN: &str = "range: is value outside [lo, hi]";
pub const DEPENDS_ON: &[&str] = &["srvcs-between", "srvcs-not"];

/// Dependency endpoints, injected as router state so tests can point them at
/// mock services.
#[derive(Clone)]
pub struct Deps {
    pub between_url: String,
    pub not_url: String,
}

#[derive(Serialize, ToSchema)]
pub struct Info {
    pub service: &'static str,
    pub concern: &'static str,
    pub depends_on: Vec<&'static str>,
}

/// `GET /` — service identity (srvcs service standard).
#[utoipa::path(get, path = "/", responses((status = 200, body = Info)))]
pub async fn index() -> Json<Info> {
    Json(Info {
        service: SERVICE,
        concern: CONCERN,
        depends_on: DEPENDS_ON.to_vec(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct EvalRequest {
    pub value: f64,
    pub lo: f64,
    pub hi: f64,
}

#[derive(Serialize, ToSchema)]
pub struct OutOfRangeResponse {
    pub value: f64,
    pub lo: f64,
    pub hi: f64,
    pub result: bool,
}

fn ok(value: f64, lo: f64, hi: f64, result: bool) -> Response {
    (
        StatusCode::OK,
        Json(json!({ "value": value, "lo": lo, "hi": hi, "result": result })),
    )
        .into_response()
}

fn degraded(dependency: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "dependency unavailable", "dependency": dependency })),
    )
        .into_response()
}

fn forward(status: u16, body: Value) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    (code, Json(body)).into_response()
}

/// A reachable dependency answered `200` but its body lacked a boolean
/// `result`. That is a contract violation we cannot recover from, so surface a
/// `500` rather than guessing.
fn malformed(dependency: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(
            json!({ "error": "dependency returned a malformed result", "dependency": dependency }),
        ),
    )
        .into_response()
}

/// Call one dependency at `url` with `body`, mapping its outcome to either the
/// parsed response body (on `200`) or an early-return `Response` the caller
/// should surface verbatim:
///
/// - unreachable / non-`200`/`422` -> `503` degraded
/// - `422` -> forwarded `422` (the dependency rejected the input)
async fn ask(url: &str, body: &Value, dependency: &str) -> Result<Value, Response> {
    match client::call(url, body).await {
        Err(DepError::Unreachable) => Err(degraded(dependency)),
        Ok((200, body)) => Ok(body),
        Ok((422, body)) => Err(forward(422, body)),
        Ok(_) => Err(degraded(dependency)),
    }
}

/// `POST /` — compute `outofrange(value, lo, hi)` by composing two primitives.
///
/// This service owns the *control flow* but delegates the actual logic to its
/// dependencies, exactly as specified:
///
/// 1. ask `srvcs-between` for `b = between(value, lo, hi)`;
/// 2. ask `srvcs-not` for `result = not(b)` — i.e. value is outside `[lo, hi]`.
///
/// Validation propagates from the dependencies: this orchestrator never calls
/// `srvcs-isnumber` directly. If a dependency is unreachable it reports itself
/// degraded (`503`); if a dependency rejects the input it forwards the `422`.
#[utoipa::path(
    post,
    path = "/",
    request_body = EvalRequest,
    responses(
        (status = 200, body = OutOfRangeResponse),
        (status = 422, description = "a dependency rejected the input (forwarded)"),
        (status = 500, description = "a dependency returned a malformed result"),
        (status = 503, description = "a dependency is unavailable")
    )
)]
pub async fn evaluate(State(deps): State<Deps>, Json(req): Json<EvalRequest>) -> Response {
    let (value, lo, hi) = (req.value, req.lo, req.hi);

    // 1. b = between(value, lo, hi)
    let between_body = match ask(
        &deps.between_url,
        &json!({ "value": value, "lo": lo, "hi": hi }),
        "srvcs-between",
    )
    .await
    {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let b = match between_body.get("result").and_then(Value::as_bool) {
        Some(b) => b,
        None => return malformed("srvcs-between"),
    };

    // 2. result = not(b)
    let not_body = match ask(&deps.not_url, &json!({ "value": b }), "srvcs-not").await {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let result = match not_body.get("result").and_then(Value::as_bool) {
        Some(r) => r,
        None => return malformed("srvcs-not"),
    };

    ok(value, lo, hi, result)
}

#[derive(OpenApi)]
#[openapi(
    paths(index, evaluate),
    components(schemas(Info, EvalRequest, OutOfRangeResponse))
)]
pub struct ApiDoc;

/// Serve OpenAPI document
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_documents_routes() {
        let doc = ApiDoc::openapi();
        let root = doc.paths.paths.get("/").expect("path / present");
        assert!(root.get.is_some());
        assert!(root.post.is_some());
    }

    #[tokio::test]
    async fn index_reports_all_dependencies() {
        let Json(info) = index().await;
        assert_eq!(info.service, "srvcs-outofrange");
        assert_eq!(info.concern, "range: is value outside [lo, hi]");
        assert_eq!(info.depends_on, vec!["srvcs-between", "srvcs-not"]);
    }
}
