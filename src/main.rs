use async_graphql::http::playground_source;
use async_graphql::{Context, EmptyMutation, EmptySubscription, Schema};
use async_std::task;
use std::env;
use tide::{
    http::{headers, mime},
    Request, Response, StatusCode,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

type Repo = async_std::sync::Mutex<git2::Repository>;

mod git;

struct MyToken(String);

struct QueryRoot;

#[async_graphql::Object]
impl QueryRoot {
    async fn page(&self, ctx: &Context<'_>) -> Page {
        let path = "";
        Page { path: path.to_owned() }
    }
}

struct Page {
    path: String,
}

#[async_graphql::Object]
impl Page {
    async fn content<'a>(&self, ctx: &'a Context<'_>) -> String {
        let repo = ctx.data::<Repo>().lock().await;
        git::get_file(&self.path, &repo).expect("error page")
    }
}

struct AppState {
    schema: Schema<QueryRoot, EmptyMutation, EmptySubscription>,
}

fn main() -> Result<()> {
    pretty_env_logger::init();
    task::block_on(run())
}

async fn run() -> Result<()> {
    let listen_addr = env::var("LISTEN_ADDR").unwrap_or_else(|_| "localhost:8000".to_owned());

    let schema = Schema::build(QueryRoot, EmptyMutation, EmptySubscription).finish();

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
