use github_actions_cache::actions::results::{
    api::v1::{CacheServiceClient, CreateCacheEntryRequest},
    entities::v1::{CacheMetadata, CacheScope},
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
    let token = std::env::var("ACTIONS_RUNTIME_TOKEN").unwrap();
    let service_url = std::env::var("ACTIONS_RESULTS_URL").unwrap();
    let repo_id = std::env::var("GITHUB_REPOSITORY_ID")
        .unwrap()
        .parse()
        .unwrap();

    let client = ClientBuilder::new(
        Url::parse(&service_url).unwrap(),
        twirp::reqwest::Client::default(),
    )
    .with(Bearer { token })
    .build()
    .unwrap();

    let resp = client
        .create_cache_entry(CreateCacheEntryRequest {
            metadata: Some(CacheMetadata {
                repository_id: repo_id,
                scope: vec![CacheScope {
                    scope: "".to_string(),
                    permission: 1 | 2, // Read | Write
                }],
            }),
            key: "foo".to_string(),
            version: "bar".to_string(),
        })
        .await
        .unwrap();

    dbg!(resp);
}
