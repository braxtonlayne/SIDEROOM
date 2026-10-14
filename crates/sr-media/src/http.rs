//! HTTP routes of the Worker API (CIN-ING-004) over [`WorkerService`].
//!
//! `router` returns a self-contained `axum::Router`; sr-server merges it once the catalog's
//! Postgres `JobStore` exists. Authentication is a bearer worker token on every route.

use crate::worker_api::{
    API_PREFIX, ApiError, ClaimRequest, CompleteRequest, FailRequest, FinishUploadRequest,
    HeartbeatRequest, InitUploadRequest, ProgressRequest, RegisterRequest, WorkerService,
};
use axum::Json;
use axum::Router;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde::Serialize;
use std::sync::Arc;

/// Largest request body: a completion with a 10-hour segment index fits comfortably.
const MAX_BODY: usize = 8 * 1024 * 1024;

struct HttpError(ApiError);

impl From<ApiError> for HttpError {
    fn from(e: ApiError) -> Self {
        Self(e)
    }
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let status =
            StatusCode::from_u16(self.0.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        // Internal details stay in the logs, never in the response.
        let message = match &self.0 {
            ApiError::Internal(_) => "internal error".to_string(),
            other => other.to_string(),
        };
        let mut res = (
            status,
            Json(ErrorBody {
                code: self.0.code(),
                message,
            }),
        )
            .into_response();
        if self.0 == ApiError::Unauthorized {
            res.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                header::HeaderValue::from_static("Bearer"),
            );
        }
        res
    }
}

type Svc = State<Arc<WorkerService>>;
type Res<T> = Result<Json<T>, HttpError>;

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let v = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = v.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then_some(token.trim())
}

async fn register(
    State(s): Svc,
    h: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> Res<crate::worker_api::RegisterResponse> {
    let c = s.authenticate(bearer(&h)).await?;
    Ok(Json(s.register(&c, req).await?))
}

async fn heartbeat(
    State(s): Svc,
    h: HeaderMap,
    Path(worker_id): Path<String>,
    Json(req): Json<HeartbeatRequest>,
) -> Res<crate::worker_api::HeartbeatResponse> {
    let c = s.authenticate(bearer(&h)).await?;
    Ok(Json(s.heartbeat(&c, &worker_id, req).await?))
}

async fn claim(
    State(s): Svc,
    h: HeaderMap,
    Path(worker_id): Path<String>,
    Json(req): Json<ClaimRequest>,
) -> Res<crate::worker_api::ClaimResponse> {
    let c = s.authenticate(bearer(&h)).await?;
    Ok(Json(s.claim(&c, &worker_id, req).await?))
}

async fn progress(
    State(s): Svc,
    h: HeaderMap,
    Path(job_id): Path<String>,
    Json(req): Json<ProgressRequest>,
) -> Res<crate::worker_api::ProgressResponse> {
    let c = s.authenticate(bearer(&h)).await?;
    Ok(Json(s.progress(&c, &job_id, req).await?))
}

async fn init_upload(
    State(s): Svc,
    h: HeaderMap,
    Path(job_id): Path<String>,
    Json(req): Json<InitUploadRequest>,
) -> Res<crate::worker_api::InitUploadResponse> {
    let c = s.authenticate(bearer(&h)).await?;
    Ok(Json(s.init_upload(&c, &job_id, req).await?))
}

async fn finish_upload(
    State(s): Svc,
    h: HeaderMap,
    Path((job_id, upload_id)): Path<(String, String)>,
    Json(req): Json<FinishUploadRequest>,
) -> Res<crate::worker_api::Ack> {
    let c = s.authenticate(bearer(&h)).await?;
    Ok(Json(s.finish_upload(&c, &job_id, &upload_id, req).await?))
}

async fn complete(
    State(s): Svc,
    h: HeaderMap,
    Path(job_id): Path<String>,
    Json(req): Json<CompleteRequest>,
) -> Res<crate::worker_api::Ack> {
    let c = s.authenticate(bearer(&h)).await?;
    Ok(Json(s.complete(&c, &job_id, req).await?))
}

async fn fail(
    State(s): Svc,
    h: HeaderMap,
    Path(job_id): Path<String>,
    Json(req): Json<FailRequest>,
) -> Res<crate::worker_api::Ack> {
    let c = s.authenticate(bearer(&h)).await?;
    Ok(Json(s.fail(&c, &job_id, req).await?))
}

/// All Worker API routes under [`API_PREFIX`].
pub fn router(service: Arc<WorkerService>) -> Router {
    let p = |tail: &str| format!("{API_PREFIX}{tail}");
    Router::new()
        .route(&p("/register"), post(register))
        .route(&p("/workers/{worker_id}/heartbeat"), post(heartbeat))
        .route(&p("/workers/{worker_id}/claim"), post(claim))
        .route(&p("/jobs/{job_id}/progress"), post(progress))
        .route(&p("/jobs/{job_id}/uploads"), post(init_upload))
        .route(
            &p("/jobs/{job_id}/uploads/{upload_id}/complete"),
            post(finish_upload),
        )
        .route(&p("/jobs/{job_id}/complete"), post(complete))
        .route(&p("/jobs/{job_id}/fail"), post(fail))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(service)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker_api::test_support::*;
    use crate::worker_api::{JobState, JobStore};
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    async fn call(
        app: &Router,
        path: &str,
        token: Option<&str>,
        body: Value,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method("POST")
            .uri(format!("{API_PREFIX}{path}"))
            .header("content-type", "application/json");
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let res = app
            .clone()
            .oneshot(req.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn reg_body() -> Value {
        serde_json::to_value(register_req()).unwrap()
    }

    #[tokio::test]
    async fn requires_a_bearer_token() {
        let h = harness();
        let app = router(Arc::new(h.svc));
        let (s, b) = call(&app, "/register", None, reg_body()).await;
        assert_eq!(
            (s, b["code"].as_str()),
            (StatusCode::UNAUTHORIZED, Some("unauthorized"))
        );
        let (s, _) = call(&app, "/register", Some("wrong"), reg_body()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let (s, b) = call(&app, "/register", Some(TOKEN), reg_body()).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(b["heartbeat_secs"], 20);
    }

    #[tokio::test]
    async fn full_proxy_job_over_http() {
        let h = harness();
        let store = h.store.clone();
        let app = router(Arc::new(h.svc));
        let (_, reg) = call(&app, "/register", Some(TOKEN), reg_body()).await;
        let wid = reg["worker_id"].as_str().unwrap().to_string();

        // Nothing queued yet.
        let (s, b) = call(
            &app,
            &format!("/workers/{wid}/claim"),
            Some(TOKEN),
            json!({}),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert!(b["job"].is_null());

        let id = store.enqueue(new_job(proxy_spec(), 9)).await.unwrap();
        let (_, b) = call(
            &app,
            &format!("/workers/{wid}/claim"),
            Some(TOKEN),
            json!({"kinds":["proxy"]}),
        )
        .await;
        assert_eq!(b["job"]["job_id"], id.as_str());
        assert_eq!(b["job"]["spec"]["kind"], "proxy");
        assert!(
            b["job"]["source"]["download_url"]
                .as_str()
                .unwrap()
                .contains("sig=S")
        );

        let (s, b) = call(
            &app,
            &format!("/jobs/{id}/progress"),
            Some(TOKEN),
            json!({"stage":"video","percent":42.5,"fps":88.0,"speed":3.2,"out_time_secs":120.0,"message":null}),
        )
        .await;
        assert_eq!((s, b["cancel"].as_bool()), (StatusCode::OK, Some(false)));

        let (s, b) = call(
            &app,
            &format!("/jobs/{id}/uploads"),
            Some(TOKEN),
            json!({"path":"proxy/proxy.mp4","size":1000}),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        let upload_id = b["upload_id"].as_str().unwrap().to_string();
        assert_eq!(b["parts"].as_array().unwrap().len(), 1);

        // Hostile path.
        let (s, b) = call(
            &app,
            &format!("/jobs/{id}/uploads"),
            Some(TOKEN),
            json!({"path":"../../../etc/cron.d/x.vtt","size":10}),
        )
        .await;
        assert_eq!(
            (s, b["code"].as_str()),
            (StatusCode::BAD_REQUEST, Some("bad_request"))
        );

        let (s, _) = call(
            &app,
            &format!("/jobs/{id}/uploads/{upload_id}/complete"),
            Some(TOKEN),
            json!({"parts":[{"part_number":1,"etag":"abc"}]}),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        let (s, _) = call(
            &app,
            &format!("/jobs/{id}/complete"),
            Some(TOKEN),
            json!({"outputs":[{"path":"proxy/proxy.mp4","size":1000}],"segment_index":[],
                   "encoder":"libx264","wall_secs":9.5,"log_tail":null}),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(
            store.get_job(&id).await.unwrap().unwrap().state,
            JobState::Succeeded
        );

        // Finished: further calls conflict.
        let (s, b) = call(
            &app,
            &format!("/jobs/{id}/fail"),
            Some(TOKEN),
            json!({"stage":"video","message":"x","log_tail":null,"retryable":true,"cancelled":false}),
        )
        .await;
        assert_eq!(
            (s, b["code"].as_str()),
            (StatusCode::CONFLICT, Some("conflict"))
        );
    }

    #[tokio::test]
    async fn malformed_json_is_a_client_error() {
        let h = harness();
        let app = router(Arc::new(h.svc));
        let (s, _) = call(&app, "/register", Some(TOKEN), json!({"name": 5})).await;
        assert!(s.is_client_error());
    }
}
