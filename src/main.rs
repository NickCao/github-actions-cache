use github_actions_cache::github::actions::results::api::v1::{
    CacheServiceClient, CreateCacheEntryRequest, FinalizeCacheEntryUploadRequest,
    GetCacheEntryDownloadUrlRequest,
};
use twirp::{
    async_trait,
    reqwest::{Request, Response},
    url::Url,
    ClientBuilder, Middleware, Next,
};

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
        next.run(req).await
    }
}

#[tokio::main]
pub async fn main() {
    let version = if std::env::var("ACTIONS_CACHE_SERVICE_V2").is_ok() {
        2
    } else {
        1
    };
    println!("version: {version}");

    let token = std::env::var("ACTIONS_RUNTIME_TOKEN").unwrap();
    let service_url = std::env::var("ACTIONS_RESULTS_URL").unwrap();

    dbg!(&service_url);

    let client = ClientBuilder::new(
        Url::parse(&service_url).unwrap(),
        twirp::reqwest::Client::default(),
    )
    .with(Bearer { token })
    .build()
    .unwrap();

    let key = "foo".to_string();
    let version = "bar".to_string();

    let resp = client
        .create_cache_entry(CreateCacheEntryRequest {
            key: key.clone(),
            version: version.clone(),
            ..Default::default()
        })
        .await
        .unwrap();

    if !resp.ok {
        panic!("failed to create cache entry");
    }

    // TODO: upload file to resp.signed_upload_url;

    let resp = client
        .finalize_cache_entry_upload(FinalizeCacheEntryUploadRequest {
            key: key.clone(),
            version: version.clone(),
            size_bytes: 100, // FIXME
            ..Default::default()
        })
        .await
        .unwrap();

    if !resp.ok {
        panic!("failed to finalize cache entry");
    }

    let resp = client
        .get_cache_entry_download_url(GetCacheEntryDownloadUrlRequest {
            key: key.clone(),
            restore_keys: vec![],
            version: version.clone(),
            ..Default::default()
        })
        .await
        .unwrap();

    if !resp.ok {
        panic!("failed to get cache entry download url");
    }

    dbg!(resp);
}
