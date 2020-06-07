use async_graphql::http::playground_source;
use async_graphql::{Context, EmptyMutation, EmptySubscription, Schema, FieldResult};
use async_std::task;
use std::env;
use tide::{
    http::{headers, mime},
    Request, Response, StatusCode,
};
use serde_derive::{Deserialize, Serialize};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

type Repo = async_std::sync::Mutex<git2::Repository>;

mod git;

struct QueryRoot;

#[async_graphql::Object]
impl QueryRoot {
    async fn page(&self, _ctx: &Context<'_>, path: String) -> Page {
        Page { path }
    }
    async fn pages(&self, ctx: &Context<'_>) -> Vec<Page> {
        let repo = ctx.data::<Repo>().lock().await;
        let paths = git::get_dir("files", &repo).expect("error page");
        paths.into_iter().map(|path| Page { path }).collect()
    }
}

struct Page {
    path: String,
}

#[derive(Deserialize, Serialize)]
struct Metadata {
    title: String,
    path: String,
}

#[async_graphql::Object]
impl Metadata {
    async fn title(&self) -> String {
        self.title.clone()
    }
    async fn path(&self) -> String {
        self.path.clone()
    }
}

#[async_graphql::Object]
impl Page {
    async fn content<'a>(&self, ctx: &'a Context<'_>) -> FieldResult<String> {
        let repo = ctx.data::<Repo>().lock().await;
        Ok(git::get_file(&format!("files/{}",&self.path), &repo)?)
    }
    async fn meta<'a>(&self, ctx: &'a Context<'_>) -> FieldResult<Metadata> {
        let repo = ctx.data::<Repo>().lock().await;
        let content = git::get_file(&format!("meta/{}.json", &self.path), &repo)?;
        Ok(serde_json::from_str(&content)?)
    }
}

struct MutationRoot;

#[async_graphql::Object]
impl MutationRoot {
    async fn commit(&self, ctx: &Context<'_>, info: git::CommitInfo, updated_files: Vec<git::StagedFile>, removed_files: Vec<String>) -> FieldResult<bool> {
        let repo = ctx.data::<Repo>().lock().await;
        git::commit_files(&info, &updated_files, &removed_files, &repo)?;
        Ok(true)
    }
}

struct AppState {
    schema: Schema<QueryRoot, MutationRoot, EmptySubscription>,
}

fn main() -> Result<()> {
    pretty_env_logger::init();
    task::block_on(run())
}

async fn run() -> Result<()> {
    let listen_addr = env::var("LISTEN_ADDR").unwrap_or_else(|_| "localhost:8000".to_owned());

    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription).finish();

    println!("Playground: http://{}", listen_addr);

    let app_state = AppState { schema };
    let mut app = tide::with_state(app_state);

    async fn graphql(req: Request<AppState>) -> tide::Result<Response> {
        let schema = req.state().schema.clone();

        async_graphql_tide::graphql(req, schema, |mut query_builder| {
            query_builder = query_builder.data(Repo::new(git::get_repo("repo").expect("no repo")));
            query_builder
        })
        .await
    }

    app.at("/graphql").post(graphql).get(graphql);
    app.at("/").get(|_| async move {
        let resp = Response::new(StatusCode::Ok)
            .body_string(playground_source("/graphql", None))
            .set_header(headers::CONTENT_TYPE, mime::HTML.to_string());

        Ok(resp)
    });

    app.listen(listen_addr).await?;

    Ok(())
}
