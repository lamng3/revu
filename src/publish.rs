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
    let (title, body) = pr_title_and_body(repo);
    let mut cmd = Command::new("gh");
    cmd.current_dir(repo)
        .args(["pr", "create", "--web=false", "--title", &title, "--body", &body]);
    let out = cmd.output()?;
    if !out.status.success() {
        // Fall back to --fill if the structured form fails for any reason
        // (e.g. upstream branch not yet pushed, auto-tracked). Surface the
        // original error if fill also fails.
        let original = String::from_utf8_lossy(&out.stderr).to_string();
        let fallback = Command::new("gh")
            .current_dir(repo)
            .args(["pr", "create", "--fill", "--web=false"])
            .output()?;
        if !fallback.status.success() {
            return Err(anyhow!(
                "gh pr create failed: {}{}",
                original,
                String::from_utf8_lossy(&fallback.stderr)
            ));
        }
    }
    current_pr_number(repo).ok_or_else(|| anyhow!("created PR but could not determine number"))
}

fn pr_title_and_body(repo: &Path) -> (String, String) {
    // Title: first line of the newest commit on this branch.
    let title = run_git(repo, &["log", "-1", "--pretty=%s"]).unwrap_or_else(|| "revu: review".into());

    // Body: list of commits on this branch that aren't on the base, plus a
    // short diff summary. Falls back gracefully if `origin/HEAD` isn't known.
    let base = run_git(repo, &["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"])
        .map(|s| s)
        .unwrap_or_else(|| "origin/main".into());
    let commits = run_git(repo, &["log", "--pretty=- %s", &format!("{base}..HEAD")])
        .unwrap_or_default();
    let shortstat = run_git(repo, &["diff", "--shortstat", &format!("{base}...HEAD")])
        .unwrap_or_default();
    let branch = run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|| "HEAD".into());

    let mut body = String::new();
    body.push_str("## Summary\n\n");
    if commits.trim().is_empty() {
        body.push_str(&format!("Changes on `{branch}`.\n"));
    } else {
        body.push_str(&commits);
        body.push('\n');
    }
    if !shortstat.trim().is_empty() {
        body.push_str(&format!("\n**Diff:**{shortstat}\n"));
    }
    body.push_str("\n## Test plan\n\n- [ ] Manual smoke test\n\n");
    body.push_str("🦞 Drafted by revu — `:pr`\n");

    (title, body)
}

fn run_git(repo: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").current_dir(repo).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
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
