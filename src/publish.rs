use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;

use crate::comments::{Comment, Side};

pub fn gh_available() -> bool {
    Command::new("gh").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

pub fn current_pr_number(repo: &Path) -> Option<u64> {
    let out = Command::new("gh")
        .current_dir(repo)
        .args(["pr", "view", "--json", "number", "-q", ".number"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    s.parse().ok()
}

pub fn create_pr(repo: &Path) -> Result<u64> {
    let out = Command::new("gh")
        .current_dir(repo)
        .args(["pr", "create", "--fill", "--web=false"])
        .output()?;
    if !out.status.success() {
        return Err(anyhow!("gh pr create failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    current_pr_number(repo).ok_or_else(|| anyhow!("created PR but could not determine number"))
}

pub fn post_review_comment(repo: &Path, pr: u64, c: &Comment) -> Result<()> {
    let side = match c.side {
        Side::Left => "LEFT",
        Side::Right => "RIGHT",
    };
    let mut args = vec![
        "api".to_string(),
        "-X".to_string(),
        "POST".to_string(),
        format!("repos/{{owner}}/{{repo}}/pulls/{}/comments", pr),
        "-f".to_string(),
        format!("body={}", c.body),
        "-f".to_string(),
        format!("commit_id={}", c.commit_sha),
        "-f".to_string(),
        format!("path={}", c.file),
        "-F".to_string(),
        format!("line={}", c.line),
        "-f".to_string(),
        format!("side={}", side),
    ];
    if let Some(start_line) = c.start_line {
        let start_side = match c.start_side.unwrap_or(c.side) {
            Side::Left => "LEFT",
            Side::Right => "RIGHT",
        };
        args.push("-F".to_string());
        args.push(format!("start_line={}", start_line));
        args.push("-f".to_string());
        args.push(format!("start_side={}", start_side));
    }
    let out = Command::new("gh").current_dir(repo).args(&args).output()?;
    if !out.status.success() {
        return Err(anyhow!("gh api failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(())
}
