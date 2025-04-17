use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::HeaderMap,
    response::Result,
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};
use github_actions_cache::github::actions::results::api::v1::{
    CacheServiceClient, CreateCacheEntryRequest, FinalizeCacheEntryUploadRequest,
    GetCacheEntryDownloadUrlRequest,
};
use rand::RngCore;
use reqwest::{header, StatusCode};
use tokio::net::TcpListener;
use twirp::{
    async_trait,
    reqwest::{Request, Response},
    url::Url,
    Client, ClientBuilder, Middleware, Next,
};

const ACTIONS_CACHE_SERVICE_V2: &str = "ACTIONS_CACHE_SERVICE_V2";
const ACTIONS_RUNTIME_TOKEN: &str = "ACTIONS_RUNTIME_TOKEN";
const ACTIONS_RESULTS_URL: &str = "ACTIONS_RESULTS_URL";

struct Bearer {
    token: String,
}

#[async_trait::async_trait]
impl Middleware for Bearer {
    async fn handle(&self, mut req: Request, next: Next<'_>) -> twirp::client::Result<Response> {
        req.headers_mut().append(
            "Authorization",
            format!("Bearer {0}", self.token).try_into()?,
        );
        req.headers_mut()
            .append("User-Agent", "actions/cache-4.0.3".try_into()?);
        next.run(req).await
    }
}

struct AppState {
    client: Client,
    rclient: reqwest::Client,
    version: String,
}

async fn upload(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Result<impl IntoResponse> {
    let key = format!("{}-{}", path, rand::rng().next_u32());

    let content_length: i64 = headers
        .get(header::CONTENT_LENGTH)
        .ok_or((
            StatusCode::BAD_REQUEST,
            "missing content-length header in request",
        ))?
        .to_str()
        .map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                format!("invalid content-length header in request: {}", err),
            )
        })?
        .parse()
        .map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                format!("invalid content-length header in request: {}", err),
            )
        })?;

    let resp = state
        .client
        .create_cache_entry(CreateCacheEntryRequest {
            key: key.clone(),
            version: state.version.clone(),
            ..Default::default()
        })
        .await
        .map_err(|err| {
            (
                StatusCode::IM_A_TEAPOT,
                format!("unexpected error: {}", err),
            )
        })?;

    if !resp.ok {
        return Ok((
            StatusCode::IM_A_TEAPOT,
            format!("failed to create key {}", key),
        ));
    }

    state
        .rclient
        .put(&resp.signed_upload_url)
        .header(header::CONTENT_LENGTH, content_length)
        .header("x-ms-blob-type", "BlockBlob")
        .body(reqwest::Body::wrap_stream(body.into_data_stream()))
        .send()
        .await
        .map_err(|err| {
            (
                StatusCode::IM_A_TEAPOT,
                format!("unexpected error: {}", err),
            )
        })?
        .error_for_status()
        .map_err(|err| {
            (
                StatusCode::IM_A_TEAPOT,
                format!("unexpected error: {}", err),
            )
        })?;

    let res = state
        .client
        .finalize_cache_entry_upload(FinalizeCacheEntryUploadRequest {
            key: key.clone(),
            version: state.version.clone(),
            size_bytes: content_length,
            ..Default::default()
        })
        .await
        .map_err(|err| {
            (
                StatusCode::IM_A_TEAPOT,
                format!("unexpected error: {}", err),
            )
        })?;

    if !res.ok {
        Ok((
            StatusCode::IM_A_TEAPOT,
            format!("key {} creation failed", key),
        ))
    } else {
        Ok((StatusCode::CREATED, format!("key {} created", key)))
    }
}

async fn download(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Result<impl IntoResponse> {
    let resp = state
        .client
        .get_cache_entry_download_url(GetCacheEntryDownloadUrlRequest {
            key: key.clone(),
            restore_keys: vec![key.clone()],
            version: state.version.clone(),
            ..Default::default()
        })
        .await
        .map_err(|err| {
            (
                StatusCode::IM_A_TEAPOT,
                format!("unexpected error: {}", err),
            )
        })?;

    if resp.ok {
        Ok(Redirect::temporary(&resp.signed_download_url).into_response())
    } else {
        Ok((StatusCode::NOT_FOUND, format!("key {} not found", key)).into_response())
    }
}

#[tokio::main]
pub async fn main() {
    if std::env::var(ACTIONS_CACHE_SERVICE_V2).is_err() {
        unimplemented!()
    };

    let token = std::env::var(ACTIONS_RUNTIME_TOKEN).unwrap();
    let service_url = std::env::var(ACTIONS_RESULTS_URL).unwrap();

    let client = ClientBuilder::new(
        Url::parse(&service_url).unwrap().join("twirp/").unwrap(),
        twirp::reqwest::Client::default(),
    )
    .with(Bearer {
        token: token.clone(),
    })
    .build()
    .unwrap();

    let app = Router::new()
        .route("/{*path}", get(download).put(upload))
        .with_state(Arc::new(AppState {
            client,
            rclient: reqwest::ClientBuilder::default().build().unwrap(),
            version: "87428fc522803d31065e7bce3cf03fe475096631e5e07bbd7a0fde60c4cf25c7".to_string(),
        }));

    axum::serve(TcpListener::bind("127.0.0.1:3000").await.unwrap(), app)
        .await
        .unwrap();
}
