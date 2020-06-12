use async_graphql::http::playground_source;
use async_graphql::{EmptySubscription, Schema};
use async_std::task;
use std::env;
use tide::{
    http::{headers, mime},
    Request, Response, StatusCode,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

mod git;
mod graphql;

use graphql::{Repo, QueryRoot, MutationRoot};

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
