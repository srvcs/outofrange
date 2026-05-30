use axum::body::Body;
use axum::extract::Json as AxumJson;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router as AxumRouter};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use srvcs_outofrange::{api::Deps, health, router, telemetry};
use tower::ServiceExt;

const DEAD_URL: &str = "http://127.0.0.1:1";

/// Spawn a *computing* mock `srvcs-between`: reads `{"value": v, "lo": lo,
/// "hi": hi}` and returns `{"result": lo <= v <= hi}` — the real closed-interval
/// membership test. The orchestration is genuinely driven by this answer rather
/// than a canned value.
async fn spawn_between() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let v = body.get("value").and_then(Value::as_f64).unwrap_or(0.0);
            let lo = body.get("lo").and_then(Value::as_f64).unwrap_or(0.0);
            let hi = body.get("hi").and_then(Value::as_f64).unwrap_or(0.0);
            Json(json!({ "result": lo <= v && v <= hi }))
        }),
    );
    serve(app).await
}

/// Spawn a *computing* mock `srvcs-not`: reads `{"value": b}` and returns
/// `{"result": !b}` — the real boolean negation.
async fn spawn_not() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|AxumJson(body): AxumJson<Value>| async move {
            let b = body.get("value").and_then(Value::as_bool).unwrap_or(false);
            Json(json!({ "result": !b }))
        }),
    );
    serve(app).await
}

/// Spawn a mock returning a fixed status + body (used for error-path tests).
async fn spawn_fixed(status: StatusCode, body: Value) -> String {
    let app = AxumRouter::new().route(
        "/",
        post(move || {
            let body = body.clone();
            async move { (status, Json(body)) }
        }),
    );
    serve(app).await
}

async fn serve(app: AxumRouter) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn app(between_url: &str, not_url: &str) -> axum::Router {
    router(
        telemetry::metrics_handle_for_tests(),
        Deps {
            between_url: between_url.to_string(),
            not_url: not_url.to_string(),
        },
    )
}

async fn outofrange(
    between_url: &str,
    not_url: &str,
    value: f64,
    lo: f64,
    hi: f64,
) -> (StatusCode, Value) {
    let res = app(between_url, not_url)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "value": value, "lo": lo, "hi": hi }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn status_of(uri: &str) -> StatusCode {
    app(DEAD_URL, DEAD_URL)
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

// --- Standard endpoints. ---

#[tokio::test]
async fn healthz_ok() {
    assert_eq!(status_of("/healthz").await, StatusCode::OK);
}

#[tokio::test]
async fn readyz_reflects_state() {
    health::set_ready(true);
    assert_eq!(status_of("/readyz").await, StatusCode::OK);
}

#[tokio::test]
async fn metrics_ok() {
    assert_eq!(status_of("/metrics").await, StatusCode::OK);
}

#[tokio::test]
async fn openapi_ok() {
    assert_eq!(status_of("/openapi.json").await, StatusCode::OK);
}

#[tokio::test]
async fn generates_request_id_when_absent() {
    let res = app(DEAD_URL, DEAD_URL)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        res.headers().contains_key("x-request-id"),
        "response must carry a generated x-request-id"
    );
}

#[tokio::test]
async fn index_reports_identity() {
    let res = app(DEAD_URL, DEAD_URL)
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["service"], "srvcs-outofrange");
    assert_eq!(body["concern"], "range: is value outside [lo, hi]");
    assert_eq!(body["depends_on"], json!(["srvcs-between", "srvcs-not"]));
}

// --- Correctness cases, against the computing mocks. ---

#[tokio::test]
async fn outofrange_15_0_10_is_true() {
    let (b, n) = (spawn_between().await, spawn_not().await);
    let (status, body) = outofrange(&b, &n, 15.0, 0.0, 10.0).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["value"], 15.0);
    assert_eq!(body["lo"], 0.0);
    assert_eq!(body["hi"], 10.0);
    // between(15,0,10)=false; not(false)=true
    assert_eq!(body["result"], true);
}

#[tokio::test]
async fn outofrange_5_0_10_is_false() {
    let (b, n) = (spawn_between().await, spawn_not().await);
    let (status, body) = outofrange(&b, &n, 5.0, 0.0, 10.0).await;
    assert_eq!(status, StatusCode::OK);
    // between(5,0,10)=true; not(true)=false
    assert_eq!(body["result"], false);
}

#[tokio::test]
async fn outofrange_boundary_is_inside() {
    let (b, n) = (spawn_between().await, spawn_not().await);
    // value == hi is inside the closed interval -> not out of range.
    let (status, body) = outofrange(&b, &n, 10.0, 0.0, 10.0).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"], false);
}

#[tokio::test]
async fn outofrange_below_lo_is_true() {
    let (b, n) = (spawn_between().await, spawn_not().await);
    let (status, body) = outofrange(&b, &n, -3.0, 0.0, 10.0).await;
    assert_eq!(status, StatusCode::OK);
    // between(-3,0,10)=false; not(false)=true
    assert_eq!(body["result"], true);
}

// --- Error / degraded paths. ---

#[tokio::test]
async fn degrades_when_between_unreachable() {
    let n = spawn_not().await;
    let (status, body) = outofrange(DEAD_URL, &n, 15.0, 0.0, 10.0).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-between");
}

#[tokio::test]
async fn degrades_when_not_unreachable() {
    // between is reachable, so the pipeline reaches the not call.
    let b = spawn_between().await;
    let (status, body) = outofrange(&b, DEAD_URL, 15.0, 0.0, 10.0).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-not");
}

#[tokio::test]
async fn forwards_422_from_between() {
    let n = spawn_not().await;
    let b = spawn_fixed(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!({ "error": "value is not a number" }),
    )
    .await;
    let (status, _) = outofrange(&b, &n, 15.0, 0.0, 10.0).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn forwards_422_from_not() {
    // between computes a real result so the pipeline reaches not, which rejects
    // -> forward 422.
    let b = spawn_between().await;
    let n = spawn_fixed(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!({ "error": "value is not a boolean" }),
    )
    .await;
    let (status, _) = outofrange(&b, &n, 15.0, 0.0, 10.0).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn malformed_between_result_is_500() {
    // between answers 200 but with no boolean result -> contract violation -> 500.
    let n = spawn_not().await;
    let b = spawn_fixed(StatusCode::OK, json!({ "result": "not-a-bool" })).await;
    let (status, body) = outofrange(&b, &n, 15.0, 0.0, 10.0).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["dependency"], "srvcs-between");
}

#[tokio::test]
async fn malformed_not_result_is_500() {
    let b = spawn_between().await;
    let n = spawn_fixed(StatusCode::OK, json!({ "result": "not-a-bool" })).await;
    let (status, body) = outofrange(&b, &n, 15.0, 0.0, 10.0).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["dependency"], "srvcs-not");
}
