use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Side {
    #[serde(rename = "LEFT")]
    Left,
    #[serde(rename = "RIGHT")]
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub file: String,
    #[serde(default)]
    pub start_line: Option<u32>,
    #[serde(default)]
    pub start_side: Option<Side>,
    pub line: u32,
    pub side: Side,
    pub commit_sha: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub published: bool,
    #[serde(default)]
    pub orphaned: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommentStore {
    pub comments: Vec<Comment>,
}

pub fn revu_dir(repo: &Path) -> PathBuf {
    repo.join(".revu")
}

pub fn comments_path(repo: &Path) -> PathBuf {
    revu_dir(repo).join("comments.json")
}

pub fn ensure_dir(repo: &Path) -> Result<()> {
    let d = revu_dir(repo);
    if !d.exists() {
        fs::create_dir_all(&d)?;
    }
    let gi = d.join(".gitignore");
    if !gi.exists() {
        fs::write(&gi, "# revu: keep local drafts out of git\n*\n!.gitignore\n")?;
    }
    Ok(())
}

pub fn load(repo: &Path) -> CommentStore {
    let p = comments_path(repo);
    if !p.exists() {
        return CommentStore::default();
    }
    let Ok(data) = fs::read_to_string(&p) else {
        return CommentStore::default();
    };
    serde_json::from_str(&data).unwrap_or_default()
}

pub fn save(repo: &Path, store: &CommentStore) -> Result<()> {
    ensure_dir(repo)?;
    let p = comments_path(repo);
    let data = serde_json::to_string_pretty(store)?;
    fs::write(p, data)?;
    Ok(())
}

pub fn new_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("c_{n:x}")
}
