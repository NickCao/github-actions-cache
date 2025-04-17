use axum::{
    body::Body,
    extract::{Path, State},
    http::HeaderMap,
    response::Result,
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};
use clap::Parser;
use github_actions_cache::github::actions::results::api::v1::{
    CacheServiceClient, CreateCacheEntryRequest, FinalizeCacheEntryUploadRequest,
    GetCacheEntryDownloadUrlRequest,
};
use rand::RngCore;
use reqwest::{header, StatusCode};
use std::{error::Error, sync::Arc};
use tokio::net::TcpListener;
use twirp::{url::Url, Client, ClientBuilder};

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

#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(env = "ACTIONS_RUNTIME_TOKEN")]
    actions_runtime_token: String,
    #[arg(env = "ACTIONS_RESULTS_URL")]
    actions_results_url: Url,
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let client = ClientBuilder::new(
        args.actions_results_url.join("twirp/")?,
        twirp::reqwest::ClientBuilder::default()
            .default_headers(reqwest::header::HeaderMap::from_iter([(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {0}", args.actions_runtime_token).try_into()?,
            )]))
            .build()?,
    )
    .build()?;

    let app = Router::new()
        .route("/{*path}", get(download).put(upload))
        .with_state(Arc::new(AppState {
            client,
            rclient: reqwest::ClientBuilder::default().build()?,
            // sha256(magic-nix-cache)
            version: "b670b214c5d50284dd81a9313516774823699df8ea28162b69ecda3f4362d9bf".to_string(),
        }));

    Ok(axum::serve(TcpListener::bind("127.0.0.1:3000").await?, app).await?)
}
