use crate::git;
use async_graphql::{Context, FieldResult};

use serde_derive::{Deserialize, Serialize};

pub type Repo = async_std::sync::Mutex<git2::Repository>;

pub struct QueryRoot;

#[async_graphql::Object]
impl QueryRoot {
    async fn page(&self, _ctx: &Context<'_>, path: String) -> Page {
        Page { path }
    }
    async fn pages(&self, ctx: &Context<'_>) -> FieldResult<Vec<Page>> {
        let repo = ctx.data::<Repo>().lock().await;
        let paths = git::get_dir("files", &repo)?;
        Ok(paths.into_iter().map(|path| Page { path }).collect())
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
        Ok(git::get_file(&format!("files/{}", &self.path), &repo)?)
    }
    async fn meta<'a>(&self, ctx: &'a Context<'_>) -> FieldResult<Metadata> {
        let repo = ctx.data::<Repo>().lock().await;
        let content = git::get_file(&format!("meta/{}.json", &self.path), &repo)?;
        Ok(serde_json::from_str(&content)?)
    }
}

pub struct MutationRoot;

#[async_graphql::InputObject]
struct PageUpdate {
    content: String,
    title: String,
    path: String,
}

#[async_graphql::Object]
impl MutationRoot {
    async fn commit(
        &self,
        ctx: &Context<'_>,
        message: String,
        updated: Vec<PageUpdate>,
        removed: Vec<String>,
    ) -> FieldResult<bool> {
        let repo = ctx.data::<Repo>().lock().await;
        let info = git::CommitInfo {
            message,
            author: "test@peori.space".to_owned(),
        };
        let updated_files = updated
            .into_iter()
            .flat_map(|pu| {
                vec![
                    git::StagedFile {
                        content: pu.content.clone(),
                        path: format!("files/{}", pu.path),
                    },
                    git::StagedFile {
                        path: format!("meta/{}.json", pu.path),
                        content: serde_json::to_string(&Metadata {
                            title: pu.title,
                            path: pu.path,
                        })
                        .expect("cannot serialize"),
                    },
                ]
            })
            .collect::<Vec<git::StagedFile>>();
        let removed_files = removed
            .into_iter()
            .flat_map(|r| vec![format!("files/{}", r), format!("meta/{}.json", r)])
            .collect::<Vec<String>>();
        git::commit_files(&info, &updated_files, &removed_files, &repo)?;
        Ok(true)
    }
}
