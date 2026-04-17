use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::Command;

pub fn repo_root(cwd: Option<&PathBuf>) -> Result<PathBuf> {
    let mut cmd = Command::new("git");
    if let Some(p) = cwd {
        cmd.current_dir(p);
    }
    let out = cmd.args(["rev-parse", "--show-toplevel"]).output()?;
    if !out.status.success() {
        return Err(anyhow!(String::from_utf8_lossy(&out.stderr).to_string()));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        return Err(anyhow!("empty git root"));
    }
    Ok(PathBuf::from(s))
}

pub fn head_sha(repo: &PathBuf) -> Result<String> {
    let out = Command::new("git").current_dir(repo).args(["rev-parse", "HEAD"]).output()?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn current_branch(repo: &PathBuf) -> Result<String> {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()?;
    if !out.status.success() {
        return Err(anyhow!(String::from_utf8_lossy(&out.stderr).to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}


pub fn default_remote_head(repo: &PathBuf) -> Option<String> {
    // Try origin/HEAD first
    let out = Command::new("git")
        .current_dir(repo)
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output()
        .ok()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        // refs/remotes/origin/main -> origin/main
        if let Some(rest) = s.strip_prefix("refs/remotes/") {
            return Some(rest.to_string());
        }
    }
    // Fallback: try origin/main or origin/master
    for cand in ["origin/main", "origin/master"] {
        let ok = Command::new("git")
            .current_dir(repo)
            .args(["rev-parse", "--verify", cand])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            return Some(cand.to_string());
        }
    }
    None
}

pub fn merge_base(repo: &PathBuf, a: &str, b: &str) -> Result<String> {
    let out = Command::new("git").current_dir(repo).args(["merge-base", a, b]).output()?;
    if !out.status.success() {
        return Err(anyhow!(String::from_utf8_lossy(&out.stderr).to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn diff_range(repo: &PathBuf, base: &str, tip: &str) -> Result<String> {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["diff", "--no-color", "-U3", &format!("{base}..{tip}")])
        .output()?;
    if !out.status.success() {
        return Err(anyhow!(String::from_utf8_lossy(&out.stderr).to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn diff_against(repo: &PathBuf, base: &str) -> Result<String> {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["diff", "--no-color", "-U3", base])
        .output()?;
    if !out.status.success() {
        return Err(anyhow!(String::from_utf8_lossy(&out.stderr).to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn working_tree_diff(repo: &PathBuf) -> Result<String> {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["diff", "--no-color", "-U3", "HEAD"])
        .output()?;
    if !out.status.success() {
        return Err(anyhow!(String::from_utf8_lossy(&out.stderr).to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub struct DiffContext {
    pub tip_sha: String,
    pub base_ref: String,
    pub raw: String,
}


pub fn load_diff(repo: &PathBuf, base_ref_override: Option<String>) -> Result<DiffContext> {
    let tip_sha = head_sha(repo)?;
    let base_ref = base_ref_override.or_else(|| default_remote_head(repo));

    if let Some(base_ref) = base_ref {
        if let Ok(base_sha) = merge_base(repo, &base_ref, "HEAD") {
            if let Ok(raw) = diff_against(repo, &base_sha) {
                if !raw.trim().is_empty() {
                    return Ok(DiffContext {
                        tip_sha,
                        base_ref: format!("{base_ref} (merge-base)"),
                        raw,
                    });
                }
            }
            if base_sha != tip_sha {
                if let Ok(raw) = diff_range(repo, &base_sha, &tip_sha) {
                    if !raw.trim().is_empty() {
                        return Ok(DiffContext {
                            tip_sha,
                            base_ref: format!("{base_ref} (committed)"),
                            raw,
                        });
                    }
                }
            }
        }
    }
    // Fall back to working tree
    let raw = working_tree_diff(repo)?;
    Ok(DiffContext {
        tip_sha,
        base_ref: "HEAD (working tree)".into(),
        raw,
    })
}
