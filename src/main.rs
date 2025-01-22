use std::i64;

use azure_storage_blobs::prelude::{BlobClient, BlobContentDisposition, BlobContentType};
use github_actions_cache::{
    github::actions::results::api::v1::{
        ArtifactServiceClient, CacheServiceClient, CreateArtifactRequest, CreateCacheEntryRequest,
        FinalizeArtifactRequest, FinalizeCacheEntryUploadRequest, GetCacheEntryDownloadUrlRequest,
    },
    google::protobuf::StringValue,
};
use jwt::{Claims, Header, Token};
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
        Url::parse(&service_url).unwrap().join("twirp/").unwrap(),
        twirp::reqwest::Client::default(),
    )
    .with(Bearer {
        token: token.clone(),
    })
    .build()
    .unwrap();

    let key = "foo".to_string();
    let version = "bar".to_string();

    let parsed: Token<Header, Claims, _> = Token::parse_unverified(&token).unwrap();
    for scope in parsed
        .claims()
        .private
        .get("scp")
        .unwrap()
        .to_string()
        .split(" ")
    {
        let parts: Vec<&str> = scope.split(":").collect();
        if parts.len() != 3 || parts[0] != "Actions.Results" {
            continue;
        }
        let workflow_run_backend_id = parts[1].to_string();
        let workflow_job_run_backend_id = parts[2].to_string();

        let name = "test".to_string();

        let resp = client
            .create_artifact(CreateArtifactRequest {
                workflow_run_backend_id: workflow_run_backend_id.clone(),
                workflow_job_run_backend_id: workflow_job_run_backend_id.clone(),
                name: name.clone(),
                expires_at: None,
                version: 4,
            })
            .await
            .unwrap();
        assert!(resp.ok);

        let resp = client
            .finalize_artifact(FinalizeArtifactRequest {
                workflow_run_backend_id: workflow_run_backend_id.clone(),
                workflow_job_run_backend_id: workflow_job_run_backend_id.clone(),
                name: name.clone(),
                size: i64::MAX,
                hash: Some(StringValue {
                    value: "1234".to_string(),
                }),
            })
            .await
            .unwrap();
        assert!(resp.ok);

        BlobClient::from_sas_url(&Url::parse(&resp.signed_upload_url).unwrap())
            .unwrap()
            .put_block_blob("test")
            .content_type(BlobContentType::from_static("text/plain"))
            .await
            .unwrap();
    }

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
