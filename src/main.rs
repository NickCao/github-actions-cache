use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::HeaderMap,
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
    version: String,
}

async fn upload(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> impl IntoResponse {
    let key = format!("{}-{}", path, rand::rng().next_u64());
    let resp = state
        .client
        .create_cache_entry(CreateCacheEntryRequest {
            key: key.clone(),
            version: state.version.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    if !resp.ok {
        return (StatusCode::INTERNAL_SERVER_ERROR, "")
            .into_response()
            .into_response();
    }

    let res = reqwest::ClientBuilder::default()
        .build()
        .unwrap()
        .put(&resp.signed_upload_url)
        .header(
            header::CONTENT_LENGTH,
            headers.get(header::CONTENT_LENGTH).unwrap(),
        )
        // .headers(headers.clone())
        .header("x-ms-blob-type", "BlockBlob")
        .body(reqwest::Body::wrap_stream(body.into_data_stream()))
        .send()
        .await
        .unwrap();

    // TODO: check

    state
        .client
        .finalize_cache_entry_upload(FinalizeCacheEntryUploadRequest {
            key: key.clone(),
            version: state.version.clone(),
            size_bytes: headers
                .get(header::CONTENT_LENGTH)
                .map(|size| size.to_str().unwrap().parse().unwrap_or(0))
                .unwrap_or(0),
            ..Default::default()
        })
        .await
        .unwrap();

    (StatusCode::CREATED, "").into_response()
}

async fn download(
    State(state): State<Arc<AppState>>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let resp = state
        .client
        .get_cache_entry_download_url(GetCacheEntryDownloadUrlRequest {
            key: path.clone(),
            restore_keys: vec![path.clone()],
            version: state.version.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    if resp.ok {
        Redirect::temporary(&resp.signed_download_url).into_response()
    } else {
        (StatusCode::NOT_FOUND, "").into_response()
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
            version: "87428fc522803d31065e7bce3cf03fe475096631e5e07bbd7a0fde60c4cf25c7".to_string(),
        }));

    axum::serve(TcpListener::bind("127.0.0.1:3000").await.unwrap(), app)
        .await
        .unwrap();
}
