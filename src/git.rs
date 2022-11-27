use git2::{Repository, Signature};
use tracing::info;

pub use git2::Error;
pub type Result<T> = std::result::Result<T, Error>;

pub fn get_repo(path: &str) -> Result<Repository> {
    Repository::open_bare(path).or_else(|_| {
        info!("Creating bare repo at {}", path);
        Repository::init_bare(path)
    })
}

pub fn get_file(path: &str, repo: &Repository) -> Result<String> {
    let obj = repo.revparse_single(&format!("master:{}", path))?;
    let blob = obj.peel_to_blob()?;
    let content = std::str::from_utf8(blob.content()).expect("not utf8");
    Ok(content.to_owned())
}
pub fn get_dir(path: &str, repo: &Repository) -> Result<Vec<String>> {
    let obj = repo.revparse_single(&format!("master:{}", path))?;
    let tree = obj.peel_to_tree()?;
    Ok(tree
        .iter()
        .map(|e| e.name().map(|s| s.to_owned()))
        .filter_map(|e| e)
        .collect())
}

#[derive(serde::Deserialize)]
pub struct CommitInfo {
    pub message: String,
    pub author: String,
    pub email: String,
}

#[derive(serde::Deserialize, Clone)]
pub struct StagedFile {
    pub content: String,
    pub path: String,
}

pub fn commit_files(
    info: &CommitInfo,
    updated_files: &[StagedFile],
    removed_files: &[String],
    repo: &Repository,
) -> Result<()> {
    let obj = repo.revparse_single("master:")?;
    let mut tree = obj.peel_to_tree()?;

    for f in updated_files {
        tree = stage_file(f, &tree, repo)?;
    }

    for f in removed_files {
        tree = remove_file(f, &tree, repo)?;
    }

    let sig = Signature::now(&info.author, &info.email)?;
    let branch = repo.find_branch("master", git2::BranchType::Local)?;
    repo.commit(
        branch.get().name(),
        &sig,
        &sig,
        &info.message,
        &tree,
        &[&branch.get().peel_to_commit()?],
    )?;

    Ok(())
}

fn stage_file<'a>(
    file: &StagedFile,
    tree: &git2::Tree<'a>,
    repo: &'a Repository,
) -> Result<git2::Tree<'a>> {
    let blob = repo.blob(file.content.as_bytes())?;

    let path = std::path::Path::new(&file.path);
    let mut oid = blob;
    let mut mode = 0o100_644;
    let mut name = path.file_name().expect("no filename");
    for comp in path.ancestors().skip(1) {
        let dirtree = if comp.file_name().is_none() {
            Some(tree.clone())
        } else if let Ok(entry) = tree.get_path(comp) {
            Some(entry.to_object(&repo).and_then(|t| t.peel_to_tree())?)
        } else {
            None
        };
        let mut builder = repo.treebuilder(dirtree.as_ref())?;
        builder.insert(name, oid, mode)?;
        oid = builder.write()?;
        name = if let Some(name) = comp.file_name() {
            name
        } else {
            break;
        };
        mode = 0o040_000;
    }

    let tree = repo.find_tree(oid)?;
    Ok(tree)
}

fn remove_file<'a>(
    file: &str,
    tree: &git2::Tree<'a>,
    repo: &'a Repository,
) -> Result<git2::Tree<'a>> {
    let path = std::path::Path::new(file);
    let parent_path = path.parent().expect("no parent");
    let parent_tree = tree.get_path(parent_path)?;
    let mut builder = repo.treebuilder(Some(
        &parent_tree
            .to_object(&repo)
            .and_then(|t| t.peel_to_tree())?,
    ))?;
    let name = path.file_name().expect("no file name");
    builder.remove(name)?;
    let mut oid = builder.write()?;

    let mut name = parent_path.file_name().expect("no parent name");
    for comp in path.ancestors().skip(2) {
        let dirtree = if comp.file_name().is_none() {
            Some(tree.clone())
        } else if let Ok(entry) = tree.get_path(comp) {
            Some(entry.to_object(&repo).and_then(|t| t.peel_to_tree())?)
        } else {
            None
        };
        let mut builder = repo.treebuilder(dirtree.as_ref())?;
        builder.insert(name, oid, 0o040_000)?;
        oid = builder.write()?;
        name = if let Some(name) = comp.file_name() {
            name
        } else {
            break;
        };
    }

    let tree = repo.find_tree(oid)?;
    Ok(tree)
}
