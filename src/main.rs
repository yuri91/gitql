use std::sync::{Arc, Mutex};
use axum::{
    routing::{get, post},
    extract::{State, Path, Json, Extension},
    Router,
    response::{IntoResponse, Response},
    http::{StatusCode, Request},
    middleware::{Next, self},
};

mod git;

#[derive(Clone)]
struct AppState {
    repo: Arc<Mutex<git2::Repository>>,
}

#[derive(thiserror::Error, Debug)]
enum AppError {
    #[error("git error: {0}")]
    Git(#[from] git::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let err = format!("Server error: {}", self);
        (StatusCode::INTERNAL_SERVER_ERROR, err).into_response()
    }
}

type AppResult<T> = Result<T, AppError>;

#[derive(Clone)]
struct UserInfo {
    user: String,
    email: String,
}

async fn auth<B>(mut req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
    let remote_user = req.headers()
        .get("RemoteUser")
        .and_then(|header| header.to_str().ok()).map(|s| s.to_owned());
    let remote_email = req.headers()
        .get("RemoteEmail")
        .and_then(|header| header.to_str().ok()).map(|s| s.to_owned());

    match (remote_user, remote_email) {
        (Some(user), Some(email)) => {

            req.extensions_mut().insert(UserInfo{user, email});
            Ok(next.run(req).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn fetch_file(State(state): State<AppState>, Path(path): Path<String>) -> AppResult<String> {
    let repo = state.repo.lock().expect("cannot lock mutex");
    let f = git::get_file(&path, &repo)?;
    Ok(f)
}

async fn fetch_dir(State(state): State<AppState>, Path(path): Path<String>) -> AppResult<String> {
    let repo = state.repo.lock().expect("cannot lock mutex");
    let d = git::get_dir(&path, &repo)?;
    let res = d.join("\n");
    Ok(res)
}

async fn fetch_root(State(state): State<AppState>, Extension(user_info): Extension<UserInfo>) -> AppResult<String> {
    let repo = state.repo.lock().expect("cannot lock mutex");
    let d = git::get_dir("", &repo)?;
    let res = d.join("\n");
    Ok(res)
}

#[derive(serde::Deserialize, Clone)]
struct Commit {
    message: String,
    added: Vec<git::StagedFile>,
    removed: Vec<String>,
}

async fn commit(State(state): State<AppState>, Extension(user_info): Extension<UserInfo>, Json(data): Json<Commit>) -> AppResult<()> {
    let repo = state.repo.lock().expect("cannot lock mutex");
    let info = git::CommitInfo {
        message: data.message,
        author: user_info.user,
        email: user_info.email,
    };
    git::commit_files(&info, &data.added, &data.removed, &repo)?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let state = AppState {
        repo: Arc::new(Mutex::new(git::get_repo("repo")?)),
    };

    let app = Router::new()
        .route("/fetch_file/*path", get(fetch_file))
        .route("/fetch_dir/*path", get(fetch_dir))
        .route("/fetch_dir/", get(fetch_root))
        .route("/commit", post(commit))
        .layer(middleware::from_fn(auth))
        .with_state(state);

    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}
