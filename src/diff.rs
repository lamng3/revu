#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Add,
    Del,
    HunkHeader,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub start_idx: usize,
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub lines: Vec<DiffLine>,
    pub hunks: Vec<Hunk>,
    pub binary: bool,
}

pub fn parse_unified(raw: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut cur: Option<FileDiff> = None;
    let mut old_ln: u32 = 0;
    let mut new_ln: u32 = 0;

    for line in raw.split('\n') {
        if line.starts_with("diff --git ") {
            if let Some(f) = cur.take() {
                files.push(f);
            }
            let path = line.split_whitespace().last().unwrap_or("").trim_start_matches("b/").to_string();
            cur = Some(FileDiff {
                path,
                old_path: None,
                lines: Vec::new(),
                hunks: Vec::new(),
                binary: false,
            });
            continue;
        }
        let Some(f) = cur.as_mut() else { continue };
        if line.starts_with("--- a/") {
            f.old_path = Some(line.trim_start_matches("--- a/").to_string());
            continue;
        }
        if line.starts_with("+++ b/") {
            f.path = line.trim_start_matches("+++ b/").to_string();
            continue;
        }
        if line.starts_with("--- ") || line.starts_with("+++ ") || line.starts_with("index ")
            || line.starts_with("new file") || line.starts_with("deleted file")
            || line.starts_with("similarity ") || line.starts_with("rename ")
            || line.starts_with("old mode") || line.starts_with("new mode")
        {
            continue;
        }
        if line.starts_with("Binary files ") {
            f.binary = true;
            continue;
        }
        if let Some(rest) = line.strip_prefix("@@ ") {
            let idx = f.lines.len();
            f.hunks.push(Hunk { start_idx: idx });
            // parse -old_start,old_count +new_start,new_count @@
            if let Some((old_s, new_s)) = parse_hunk_header(rest) {
                old_ln = old_s;
                new_ln = new_s;
            }
            f.lines.push(DiffLine {
                kind: LineKind::HunkHeader,
                old_lineno: None,
                new_lineno: None,
                text: line.to_string(),
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix('+') {
            if rest.starts_with("+ ") { /* unlikely */ }
            f.lines.push(DiffLine {
                kind: LineKind::Add,
                old_lineno: None,
                new_lineno: Some(new_ln),
                text: rest.to_string(),
            });
            new_ln += 1;
        } else if let Some(rest) = line.strip_prefix('-') {
            f.lines.push(DiffLine {
                kind: LineKind::Del,
                old_lineno: Some(old_ln),
                new_lineno: None,
                text: rest.to_string(),
            });
            old_ln += 1;
        } else if let Some(rest) = line.strip_prefix(' ') {
            f.lines.push(DiffLine {
                kind: LineKind::Context,
                old_lineno: Some(old_ln),
                new_lineno: Some(new_ln),
                text: rest.to_string(),
            });
            old_ln += 1;
            new_ln += 1;
        } else if line.is_empty() {
            // tolerate blank
        }
    }
    if let Some(f) = cur.take() {
        files.push(f);
    }
    files
}

fn parse_hunk_header(s: &str) -> Option<(u32, u32)> {
    // -old,oldc +new,newc @@
    let mut old_s = 0u32;
    let mut new_s = 0u32;
    for part in s.split_whitespace() {
        if let Some(rest) = part.strip_prefix('-') {
            old_s = rest.split(',').next()?.parse().ok()?;
        } else if let Some(rest) = part.strip_prefix('+') {
            new_s = rest.split(',').next()?.parse().ok()?;
        } else if part == "@@" {
            break;
        }
    }
    Some((old_s, new_s))
}
