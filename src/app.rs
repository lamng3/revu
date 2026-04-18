use anyhow::Result;
use chrono::Utc;
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::Instant,
};

use crate::comments::{self, Comment, CommentStore, Side};
use crate::diff::{FileDiff, LineKind};
use crate::git::{self, DiffContext};
use crate::publish;
use crate::snappy::Snappy;
use crate::voice;

#[derive(Debug, Clone)]
pub enum Mode {
    Normal,
    Command(String),
    Comment(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    Files,
    Comments,
}

pub struct ClickAreas {
    pub file_rows: Vec<(u16, u16, usize)>,
    pub folder_rows: Vec<(u16, u16, String)>,
    pub diff_rows: Vec<(u16, usize)>,
    pub comment_rows: Vec<(u16, u16, usize)>,
    pub file_panel_bounds: Option<(u16, u16, u16, u16)>,
    pub diff_panel_bounds: Option<(u16, u16, u16, u16)>,
}

impl Default for ClickAreas {
    fn default() -> Self {
        Self {
            file_rows: vec![],
            folder_rows: vec![],
            diff_rows: vec![],
            comment_rows: vec![],
            file_panel_bounds: None,
            diff_panel_bounds: None,
        }
    }
}

pub struct App {
    pub repo_root: PathBuf,
    pub branch_name: String,
    pub diff_ctx: Option<DiffContext>,
    pub files: Vec<FileDiff>,
    pub file_idx: usize,
    pub line_idx: usize,
    pub scroll: usize,
    pub review_range_anchor: Option<usize>,
    pub store: CommentStore,
    pub mode: Mode,
    pub overlay: Overlay,
    pub snappy: Snappy,
    pub status: String,
    pub recorder: Option<voice::Recorder>,
    pub recording_since: Option<Instant>,
    pub transcribe_rx: Option<Receiver<anyhow::Result<String>>>,
    pub transcribe_started: Option<Instant>,
    pub comment_cursor: usize,
    pub click: ClickAreas,
    pub diff_viewport_height: u16,
    pub base_ref: Option<String>,
    pub file_overlay_idx: usize,
    pub comment_overlay_idx: usize,
    pub file_panel_scroll: usize,
    pub file_panel_viewport_height: usize,
    pub file_overlay_scroll: usize,
    pub comment_overlay_scroll: usize,
    pub help_scroll: usize,
    pub mouse_drag_anchor: Option<usize>,
    pub collapsed_folders: BTreeSet<String>,
    pub follow_selected_file: bool,
}

impl App {
    pub fn new(repo_root: PathBuf, base_ref: Option<String>) -> Result<Self> {
        let _ = comments::ensure_dir(&repo_root);
        let store = comments::load(&repo_root);
        let snappy = Snappy::new();
        let branch_name = git::current_branch(&repo_root).unwrap_or_else(|_| "HEAD".to_string());
        Ok(Self {
            repo_root,
            branch_name,
            diff_ctx: None,
            files: Vec::new(),
            file_idx: 0,
            line_idx: 0,
            scroll: 0,
            review_range_anchor: None,
            store,
            mode: Mode::Normal,
            overlay: Overlay::None,
            snappy,
            status: "loaded review".to_string(),
            recorder: None,
            recording_since: None,
            transcribe_rx: None,
            transcribe_started: None,
            comment_cursor: 0,
            click: ClickAreas::default(),
            diff_viewport_height: 10,
            base_ref,
            file_overlay_idx: 0,
            comment_overlay_idx: 0,
            file_panel_scroll: 0,
            file_panel_viewport_height: 0,
            file_overlay_scroll: 0,
            comment_overlay_scroll: 0,
            help_scroll: 0,
            mouse_drag_anchor: None,
            collapsed_folders: BTreeSet::new(),
            follow_selected_file: true,
        })
    }

    pub fn tick(&mut self) -> bool {
        let a = self.snappy.tick();
        let b = self.poll_voice();
        a || b
    }

    pub fn current_file(&self) -> Option<&FileDiff> {
        self.files.get(self.file_idx)
    }

    pub fn repo_name(&self) -> String {
        self.repo_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("revu")
            .to_string()
    }

    pub fn base_label(&self) -> String {
        self.diff_ctx
            .as_ref()
            .map(|ctx| ctx.base_ref.clone())
            .unwrap_or_else(|| "loading".to_string())
    }

    pub fn unpublished_count(&self) -> usize {
        self.store.comments.iter().filter(|c| !c.published).count()
    }

    pub fn reload_diff(&mut self) {
        // Refresh origin refs so that a just-merged PR registers as "no
        // diff" instead of still showing the pre-merge file list. Runs in
        // the foreground but with a short timeout-ish scope; best-effort.
        let _ = std::process::Command::new("git")
            .current_dir(&self.repo_root)
            .args(["fetch", "--quiet", "--prune", "origin"])
            .output();
        let current_path = self.current_file().map(|f| f.path.clone());
        match git::load_diff(&self.repo_root, self.base_ref.clone()) {
            Ok(ctx) => {
                let files = crate::diff::parse_unified(&ctx.raw);
                let file_count = files.len();
                self.diff_ctx = Some(ctx);
                self.files = files;
                if let Some(path) = current_path {
                    if let Some(idx) = self.files.iter().position(|f| f.path == path) {
                        self.file_idx = idx;
                    } else if self.file_idx >= self.files.len() {
                        self.file_idx = 0;
                    }
                } else if self.file_idx >= self.files.len() {
                    self.file_idx = 0;
                }
                if let Some(file) = self.current_file() {
                    if self.line_idx >= file.lines.len() {
                        self.line_idx = 0;
                    }
                } else {
                    self.line_idx = 0;
                }
                self.scroll = 0;
                self.review_range_anchor = None;
                self.file_overlay_idx = self.file_idx;
                self.file_panel_scroll = self.file_panel_scroll.min(self.files.len().saturating_sub(1));
                self.follow_selected_file = true;
                self.comment_overlay_idx = self.comment_overlay_idx.min(self.store.comments.len().saturating_sub(1));
                self.file_overlay_scroll = self.file_overlay_scroll.min(self.files.len().saturating_sub(1));
                self.comment_overlay_scroll = self.comment_overlay_scroll.min(self.store.comments.len().saturating_sub(1));
                self.recompute_orphans();
                self.status = if file_count == 0 {
                    "no diff found  ·  :r reload".to_string()
                } else {
                    format!("loaded {file_count} file(s)")
                };
            }
            Err(e) => {
                self.status = format!("diff error: {e}");
            }
        }
    }

    fn recompute_orphans(&mut self) {
        let mut changed = false;
        let tip = self.diff_ctx.as_ref().map(|c| c.tip_sha.clone()).unwrap_or_default();
        for c in &mut self.store.comments {
            if c.published {
                continue;
            }
            let in_diff = self
                .files
                .iter()
                .find(|f| f.path == c.file)
                .map(|f| {
                    let end_present = f.lines.iter().any(|l| Self::comment_ends_on_line(c, l));
                    let start_present = if let Some(start_line) = c.start_line {
                        f.lines.iter().any(|l| match c.start_side.unwrap_or(c.side) {
                            Side::Right => l.new_lineno == Some(start_line) && l.kind != LineKind::Del,
                            Side::Left => l.old_lineno == Some(start_line) && l.kind != LineKind::Add,
                        })
                    } else {
                        true
                    };
                    start_present && end_present
                })
                .unwrap_or(false);
            let new_orphan = !in_diff;
            if new_orphan != c.orphaned {
                c.orphaned = new_orphan;
                changed = true;
            }
            if !new_orphan && !tip.is_empty() {
                c.commit_sha = tip.clone();
            }
        }
        if changed {
            let _ = comments::save(&self.repo_root, &self.store);
        }
    }

    pub fn move_line(&mut self, delta: i32) {
        let Some(f) = self.current_file() else {
            return;
        };
        if f.lines.is_empty() {
            return;
        }
        let new = (self.line_idx as i32 + delta).clamp(0, f.lines.len() as i32 - 1) as usize;
        self.line_idx = new;
        self.ensure_visible();
    }

    pub fn scroll_diff_view(&mut self, delta: i32) {
        let Some(file_len) = self.current_file().map(|file| file.lines.len()) else {
            return;
        };
        if file_len == 0 {
            return;
        }
        let viewport = self.diff_viewport_height.max(1) as usize;
        let max_scroll = file_len.saturating_sub(viewport);
        let next_scroll = (self.scroll as i32 + delta).clamp(0, max_scroll as i32) as usize;
        self.scroll = next_scroll;

        if self.line_idx < self.scroll {
            self.line_idx = self.scroll;
        } else {
            let bottom = self.scroll + viewport.saturating_sub(1);
            if self.line_idx > bottom {
                self.line_idx = bottom.min(file_len.saturating_sub(1));
            }
        }
    }

    pub fn ensure_visible(&mut self) {
        let h = self.diff_viewport_height as usize;
        if h == 0 {
            return;
        }
        if self.line_idx < self.scroll {
            self.scroll = self.line_idx;
        } else if self.line_idx >= self.scroll + h {
            self.scroll = self.line_idx + 1 - h;
        }
    }

    pub fn move_file(&mut self, delta: i32) {
        if self.files.is_empty() {
            return;
        }
        let new = (self.file_idx as i32 + delta).clamp(0, self.files.len() as i32 - 1) as usize;
        if new != self.file_idx {
            self.file_idx = new;
            self.file_overlay_idx = new;
            self.file_overlay_scroll = self.file_overlay_scroll.min(new);
            self.ensure_file_panel_visible();
            self.follow_selected_file = true;
            self.line_idx = 0;
            self.scroll = 0;
            self.review_range_anchor = None;
        }
    }

    pub fn goto_top(&mut self) {
        self.line_idx = 0;
        self.scroll = 0;
    }

    pub fn goto_bottom(&mut self) {
        if let Some(f) = self.current_file() {
            if !f.lines.is_empty() {
                self.line_idx = f.lines.len() - 1;
            }
        }
        self.ensure_visible();
    }

    pub fn next_hunk(&mut self) {
        let Some(f) = self.current_file() else {
            return;
        };
        let cur = self.line_idx;
        if let Some(h) = f.hunks.iter().find(|h| h.start_idx > cur) {
            self.line_idx = h.start_idx;
        } else if let Some(h) = f.hunks.last() {
            self.line_idx = h.start_idx;
        }
        self.ensure_visible();
    }

    pub fn prev_hunk(&mut self) {
        let Some(f) = self.current_file() else {
            return;
        };
        let cur = self.line_idx;
        if let Some(h) = f.hunks.iter().rev().find(|h| h.start_idx < cur) {
            self.line_idx = h.start_idx;
        } else if let Some(h) = f.hunks.first() {
            self.line_idx = h.start_idx;
        }
        self.ensure_visible();
    }

    pub fn jump_latest_diff(&mut self) {
        if let Some((last_i, _)) = self
            .files
            .iter()
            .enumerate()
            .rev()
            .find(|(_, f)| !f.hunks.is_empty())
        {
            self.file_idx = last_i;
            self.file_overlay_idx = last_i;
            let f = &self.files[last_i];
            if let Some(h) = f.hunks.last() {
                self.line_idx = h.start_idx;
                self.scroll = 0;
                self.ensure_visible();
            }
        }
    }

    pub fn jump_next_comment(&mut self) {
        let Some(file) = self.current_file() else {
            return;
        };
        let current_idx = self.line_idx;
        if let Some(next_idx) = file
            .lines
            .iter()
            .enumerate()
            .find(|(idx, line)| *idx > current_idx && self.line_has_comment(line))
            .map(|(idx, _)| idx)
            .or_else(|| {
                file.lines
                    .iter()
                    .enumerate()
                    .find(|(_, line)| self.line_has_comment(line))
                    .map(|(idx, _)| idx)
            })
        {
            self.line_idx = next_idx;
            self.ensure_visible();
        }
    }

    pub fn jump_prev_comment(&mut self) {
        let Some(file) = self.current_file() else {
            return;
        };
        let current_idx = self.line_idx;
        if let Some(prev_idx) = file
            .lines
            .iter()
            .enumerate()
            .rev()
            .find(|(idx, line)| *idx < current_idx && self.line_has_comment(line))
            .map(|(idx, _)| idx)
            .or_else(|| {
                file.lines
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, line)| self.line_has_comment(line))
                    .map(|(idx, _)| idx)
            })
        {
            self.line_idx = prev_idx;
            self.ensure_visible();
        }
    }

    pub fn open_help(&mut self) {
        self.help_scroll = 0;
        self.overlay = Overlay::Help;
    }

    pub fn toggle_files_overlay(&mut self) {
        if self.overlay == Overlay::Files {
            self.overlay = Overlay::None;
        } else {
            self.file_overlay_idx = self.file_idx;
            self.ensure_file_overlay_visible(0);
            self.overlay = Overlay::Files;
        }
    }

    pub fn toggle_comments_overlay(&mut self) {
        if self.overlay == Overlay::Comments {
            self.overlay = Overlay::None;
        } else {
            self.comment_overlay_idx = self.comment_overlay_idx.min(self.store.comments.len().saturating_sub(1));
            self.ensure_comment_overlay_visible(0);
            self.overlay = Overlay::Comments;
        }
    }

    pub fn close_overlay(&mut self) {
        self.overlay = Overlay::None;
        self.help_scroll = 0;
        self.mouse_drag_anchor = None;
    }

    pub fn overlay_move(&mut self, delta: i32) {
        match self.overlay {
            Overlay::Files => {
                if self.files.is_empty() {
                    return;
                }
                self.file_overlay_idx = (self.file_overlay_idx as i32 + delta)
                    .clamp(0, self.files.len() as i32 - 1) as usize;
                self.ensure_file_overlay_visible(8);
            }
            Overlay::Comments => {
                if self.store.comments.is_empty() {
                    return;
                }
                self.comment_overlay_idx = (self.comment_overlay_idx as i32 + delta)
                    .clamp(0, self.store.comments.len() as i32 - 1) as usize;
                self.ensure_comment_overlay_visible(8);
            }
            Overlay::Help => {
                self.help_scroll = (self.help_scroll as i32 + delta).max(0) as usize;
            }
            _ => {}
        }
    }

    pub fn activate_overlay_selection(&mut self) {
        match self.overlay {
            Overlay::Files => {
                if self.file_overlay_idx < self.files.len() {
                    self.file_idx = self.file_overlay_idx;
                    self.line_idx = 0;
                    self.scroll = 0;
                }
                self.overlay = Overlay::None;
            }
            Overlay::Comments => {
                if let Some(comment) = self.store.comments.get(self.comment_overlay_idx).cloned() {
                    self.jump_to_comment(&comment);
                }
                self.overlay = Overlay::None;
            }
            Overlay::Help => {
                self.overlay = Overlay::None;
            }
            Overlay::None => {}
        }
    }

    fn jump_to_comment(&mut self, comment: &Comment) {
        if let Some((file_idx, file)) = self
            .files
            .iter()
            .enumerate()
            .find(|(_, f)| f.path == comment.file)
        {
            self.file_idx = file_idx;
            self.file_overlay_idx = file_idx;
            if let Some(line_idx) = file.lines.iter().position(|line| match comment.side {
                Side::Right => line.new_lineno == Some(comment.line),
                Side::Left => line.old_lineno == Some(comment.line),
            }) {
                self.line_idx = line_idx;
                self.ensure_visible();
            }
        }
    }

    pub fn on_click(&mut self, col: u16, row: u16) {
        if self.overlay == Overlay::None {
            if let Some((x, y, w, h)) = self.click.file_panel_bounds {
                if col >= x && col < x + w && row >= y && row < y + h {
                    for (top, bottom, path) in &self.click.folder_rows {
                        if row >= *top && row < *bottom {
                            self.toggle_folder(path.clone());
                            return;
                        }
                    }
                    for (top, bottom, idx) in &self.click.file_rows {
                        if row >= *top && row < *bottom {
                            self.file_idx = *idx;
                            self.file_overlay_idx = *idx;
                            self.file_overlay_scroll = self.file_overlay_scroll.min(*idx);
                            self.ensure_file_panel_visible();
                            self.follow_selected_file = true;
                            self.line_idx = 0;
                            self.scroll = 0;
                            self.review_range_anchor = None;
                            return;
                        }
                    }
                }
            }

            if let Some((x, y, w, h)) = self.click.diff_panel_bounds {
                if col >= x && col < x + w && row >= y && row < y + h {
                    // While a multi-line range is active (V or active drag),
                    // clicks just move the cursor — never auto-open the
                    // editor. Press `c` to comment the selected range.
                    let range_active = self.review_range_anchor.is_some();

                    // Click on a rendered comment bubble → open that
                    // comment's editor (only when no range is active).
                    if !range_active {
                        let comment_rows = self.click.comment_rows.clone();
                        for (top, bottom, idx) in &comment_rows {
                            if row >= *top && row < *bottom {
                                self.line_idx = *idx;
                                self.ensure_visible();
                                self.begin_comment();
                                return;
                            }
                        }
                    }

                    // Plain diff row: select it. Without an active range,
                    // also open the editor if the clicked line already has
                    // a comment, so users can click-to-edit directly.
                    let diff_rows = self.click.diff_rows.clone();
                    for (r, line_idx) in &diff_rows {
                        if *r == row {
                            self.line_idx = *line_idx;
                            self.ensure_visible();
                            if !range_active {
                                let has_comment = self
                                    .current_file()
                                    .and_then(|f| f.lines.get(*line_idx).cloned())
                                    .map(|l| self.line_has_comment(&l))
                                    .unwrap_or(false);
                                if has_comment {
                                    self.begin_comment();
                                }
                            }
                            return;
                        }
                    }
                }
            }
        } else {
            match self.overlay {
                Overlay::Files => {
                    for (top, bottom, idx) in &self.click.file_rows {
                        if row >= *top && row < *bottom {
                            self.file_overlay_idx = *idx;
                            self.activate_overlay_selection();
                            return;
                        }
                    }
                }
                Overlay::Comments => {
                    for (top, bottom, idx) in &self.click.comment_rows {
                        if row >= *top && row < *bottom {
                            self.comment_overlay_idx = *idx;
                            self.activate_overlay_selection();
                            return;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub fn toggle_folder(&mut self, path: String) {
        if !self.collapsed_folders.insert(path.clone()) {
            self.collapsed_folders.remove(&path);
            self.status = format!("expanded {path}");
        } else {
            self.status = format!("collapsed {path}");
        }
        self.follow_selected_file = true;
    }

    pub fn on_drag_start(&mut self, col: u16, row: u16) {
        if self.overlay != Overlay::None {
            return;
        }
        if let Some((x, y, w, h)) = self.click.diff_panel_bounds {
            if col >= x && col < x + w && row >= y && row < y + h {
                for (r, line_idx) in &self.click.diff_rows {
                    if *r == row {
                        self.line_idx = *line_idx;
                        self.review_range_anchor = Some(*line_idx);
                        self.mouse_drag_anchor = Some(*line_idx);
                        self.ensure_visible();
                        return;
                    }
                }
            }
        }
    }

    pub fn on_drag_update(&mut self, col: u16, row: u16) {
        let Some(anchor) = self.mouse_drag_anchor else {
            return;
        };
        if let Some((x, y, w, h)) = self.click.diff_panel_bounds {
            if col >= x && col < x + w && row >= y && row < y + h {
                for (r, line_idx) in &self.click.diff_rows {
                    if *r == row {
                        self.review_range_anchor = Some(anchor);
                        self.line_idx = *line_idx;
                        self.ensure_visible();
                        return;
                    }
                }
            }
        }
    }

    pub fn on_drag_end(&mut self) {
        self.mouse_drag_anchor = None;
    }

    pub fn cancel_mouse_selection(&mut self) {
        self.mouse_drag_anchor = None;
        if self.review_range_anchor.is_some() {
            self.review_range_anchor = None;
            self.status = "review range cleared".into();
        }
    }

    pub fn seek_to_commentable_line(&mut self) {
        let Some(file) = self.current_file() else { return };
        let start = self.line_idx.min(file.lines.len().saturating_sub(1));
        let len = file.lines.len();
        for offset in 0..len {
            for &idx in &[start.saturating_add(offset).min(len - 1), start.saturating_sub(offset)] {
                if let Some(line) = file.lines.get(idx) {
                    if Self::comment_target_for_line(line).is_some() {
                        self.line_idx = idx;
                        self.review_range_anchor = None;
                        return;
                    }
                }
            }
        }
    }

    pub fn begin_comment(&mut self) {
        self.overlay = Overlay::None;
        let Some((start_idx, end_idx)) = self.selected_diff_range() else {
            return;
        };
        let Some((start_side, start_line, end_side, end_line, file)) = self.resolve_comment_range(start_idx, end_idx) else {
            self.status = "selected range is not commentable".into();
            return;
        };
        // Prefer an exact-range match (same start/end). Fall back to ANY
        // unpublished comment on this file whose range covers the current
        // anchor line so `c` reliably reopens a draft for editing.
        let existing = self
            .store
            .comments
            .iter()
            .find(|c| {
                c.file == file
                    && c.start_line == start_line
                    && c.start_side == start_side
                    && c.line == end_line
                    && c.side == end_side
                    && !c.published
            })
            .or_else(|| {
                self.store.comments.iter().find(|c| {
                    c.file == file
                        && !c.published
                        && Self::comment_contains_line(c, end_side, end_line)
                })
            })
            .map(|c| c.body.clone())
            .unwrap_or_default();
        self.comment_cursor = existing.chars().count();
        self.mode = Mode::Comment(existing);
        self.status = if let Some(start_line) = start_line {
            format!("drafting review comment on {file}:{start_line}-{end_line}")
        } else {
            format!("drafting review comment on {file}:{end_line}")
        };
    }

    pub fn save_current_comment(&mut self, body: String) {
        let body = body.trim().to_string();
        let Some((start_idx, end_idx)) = self.selected_diff_range() else {
            return;
        };
        let Some((start_side, start_line, end_side, end_line, file)) = self.resolve_comment_range(start_idx, end_idx) else {
            return;
        };
        let tip = self.diff_ctx.as_ref().map(|c| c.tip_sha.clone()).unwrap_or_default();
        let existing_idx = self
            .store
            .comments
            .iter()
            .position(|c| {
                c.file == file
                    && c.start_line == start_line
                    && c.start_side == start_side
                    && c.line == end_line
                    && c.side == end_side
                    && !c.published
            })
            .or_else(|| {
                self.store.comments.iter().position(|c| {
                    c.file == file
                        && !c.published
                        && Self::comment_contains_line(c, end_side, end_line)
                })
            });

        if body.is_empty() {
            if let Some(i) = existing_idx {
                self.store.comments.remove(i);
                self.status = "comment deleted".into();
            } else {
                self.status = "empty comment discarded".into();
            }
        } else if let Some(i) = existing_idx {
            self.store.comments[i].body = body;
            self.store.comments[i].commit_sha = tip;
            self.store.comments[i].orphaned = false;
            self.status = "comment updated".into();
        } else {
            self.store.comments.push(Comment {
                id: comments::new_id(),
                file,
                start_line,
                start_side,
                line: end_line,
                side: end_side,
                commit_sha: tip,
                body,
                created_at: Utc::now(),
                published: false,
                orphaned: false,
            });
            self.comment_overlay_idx = self.store.comments.len().saturating_sub(1);
            self.status = "comment saved locally".into();
        }
        self.review_range_anchor = None;
        let _ = comments::save(&self.repo_root, &self.store);
    }

    pub fn delete_current_comment(&mut self) {
        let Some(line) = self.current_file().and_then(|f| f.lines.get(self.line_idx)).cloned() else {
            return;
        };
        let Some(file) = self.current_file().map(|f| f.path.clone()) else {
            return;
        };
        let Some((side, ln)) = Self::comment_target_for_line(&line) else {
            self.status = "no comment target on this row".into();
            return;
        };
        if let Some(idx) = self
            .store
            .comments
            .iter()
            .position(|c| c.file == file && Self::comment_contains_line(c, side, ln) && !c.published)
        {
            self.store.comments.remove(idx);
            let _ = comments::save(&self.repo_root, &self.store);
            self.status = "review comment deleted".into();
            self.review_range_anchor = None;
        } else {
            self.status = "no saved draft comment on this line".into();
        }
    }

    pub fn line_has_comment(&self, line: &crate::diff::DiffLine) -> bool {
        self.comment_marker_for_line(line).is_some()
    }

    /// Gutter glyph for a tree-style multi-line comment span. Returns `●`
    /// for single-line comments, `┌`/`│`/`└` for the start/middle/end of a
    /// multi-line range. Caller colors it with `comment_marker_for_line`
    /// so state (draft/published/orphaned) is still conveyed.
    pub fn comment_tree_glyph_for_line(&self, line: &crate::diff::DiffLine) -> Option<char> {
        let file = self.current_file()?;
        let covering = self
            .store
            .comments
            .iter()
            .find(|c| c.file == file.path && Self::comment_covers_line(c, line))?;
        let start = covering.start_line.unwrap_or(covering.line);
        let end = covering.line;
        if start == end {
            return Some('●');
        }
        let current = match covering.side {
            Side::Right => line.new_lineno?,
            Side::Left => line.old_lineno?,
        };
        if current == start {
            Some('┌')
        } else if current == end {
            Some('└')
        } else {
            Some('│')
        }
    }

    pub fn comment_marker_for_line(&self, line: &crate::diff::DiffLine) -> Option<char> {
        let Some(file) = self.current_file() else {
            return None;
        };
        self.store
            .comments
            .iter()
            .filter(|comment| comment.file == file.path)
            .find_map(|comment| {
                let matches = Self::comment_covers_line(comment, line);
                if matches {
                    if comment.orphaned {
                        Some('!')
                    } else if comment.published {
                        Some('P')
                    } else {
                        Some('D')
                    }
                } else {
                    None
                }
            })
    }

    pub fn file_change_counts(&self, idx: usize) -> (usize, usize) {
        self.files.get(idx).map(count_changes).unwrap_or((0, 0))
    }

    pub fn scroll_file_panel(&mut self, delta: i32, max_rows: usize) {
        if max_rows == 0 {
            return;
        }
        let viewport = self.file_panel_viewport_height.max(1);
        let max_scroll = max_rows.saturating_sub(viewport);
        self.file_panel_scroll = (self.file_panel_scroll as i32 + delta).clamp(0, max_scroll as i32) as usize;
        self.follow_selected_file = false;
    }

    pub fn on_scroll(&mut self, col: u16, row: u16, delta: i32, file_tree_rows: usize) {
        if self.overlay == Overlay::Help {
            self.help_scroll = (self.help_scroll as i32 + delta).max(0) as usize;
            return;
        }
        if let Some((x, y, w, h)) = self.click.file_panel_bounds {
            if col >= x && col < x + w && row >= y && row < y + h {
                self.scroll_file_panel(delta, file_tree_rows);
                return;
            }
        }
        if let Some((x, y, w, h)) = self.click.diff_panel_bounds {
            if col >= x && col < x + w && row >= y && row < y + h {
                self.scroll_diff_view(delta * 3);
            }
        }
    }

    pub fn ensure_file_panel_visible_for_row(&mut self, selected_row: usize, total_rows: usize) {
        let height = self.file_panel_viewport_height;
        if height == 0 || total_rows == 0 {
            return;
        }
        if selected_row < self.file_panel_scroll {
            self.file_panel_scroll = selected_row;
        } else if selected_row >= self.file_panel_scroll + height {
            self.file_panel_scroll = selected_row + 1 - height;
        }
        self.file_panel_scroll = self.file_panel_scroll.min(total_rows.saturating_sub(height));
    }

    pub fn ensure_file_panel_visible(&mut self) {
        let height = self.file_panel_viewport_height;
        if height == 0 {
            return;
        }
        if self.file_idx < self.file_panel_scroll {
            self.file_panel_scroll = self.file_idx;
        } else if self.file_idx >= self.file_panel_scroll + height {
            self.file_panel_scroll = self.file_idx + 1 - height;
        }
        self.file_panel_scroll = self.file_panel_scroll.min(self.files.len().saturating_sub(height));
    }

    pub fn comment_body_for_line(&self, line: &crate::diff::DiffLine) -> Option<String> {
        let Some(file) = self.current_file() else {
            return None;
        };
        self.store
            .comments
            .iter()
            .find(|comment| comment.file == file.path && Self::comment_ends_on_line(comment, line))
            .map(|comment| comment.body.clone())
    }

    pub fn line_in_selected_range(&self, idx: usize) -> bool {
        // Only true when a multi-line range is actively anchored (V pressed
        // or mouse drag active). Without an anchor the "range" would just be
        // the current line and we don't want to highlight anything extra.
        let Some(anchor) = self.review_range_anchor else {
            return false;
        };
        let (start, end) = (anchor.min(self.line_idx), anchor.max(self.line_idx));
        idx >= start && idx <= end
    }

    pub fn multiline_active(&self) -> bool {
        self.review_range_anchor.is_some()
    }

    /// Returns the displayed line-number range (start, end) on the current
    /// side for the active multi-line selection, if any.
    pub fn multiline_range(&self) -> Option<(u32, u32)> {
        let anchor = self.review_range_anchor?;
        let file = self.current_file()?;
        let (a, b) = (anchor.min(self.line_idx), anchor.max(self.line_idx));
        let line_no = |l: &crate::diff::DiffLine| -> Option<u32> {
            l.new_lineno.or(l.old_lineno)
        };
        let start = line_no(file.lines.get(a)?)?;
        let end = line_no(file.lines.get(b)?)?;
        Some((start.min(end), start.max(end)))
    }

    pub fn toggle_review_range(&mut self) {
        if self.review_range_anchor.is_some() {
            self.review_range_anchor = None;
            self.status = "review range cleared".into();
        } else {
            self.review_range_anchor = Some(self.line_idx);
            self.status = "review range started".into();
        }
    }

    fn selected_diff_range(&self) -> Option<(usize, usize)> {
        let Some(file) = self.current_file() else {
            return None;
        };
        if file.lines.is_empty() {
            return None;
        }
        let anchor = self.review_range_anchor.unwrap_or(self.line_idx);
        Some((anchor.min(self.line_idx), anchor.max(self.line_idx)))
    }

    fn resolve_comment_range(
        &self,
        start_idx: usize,
        end_idx: usize,
    ) -> Option<(Option<Side>, Option<u32>, Side, u32, String)> {
        let file = self.current_file()?;
        let anchor_line = file.lines.get(self.review_range_anchor.unwrap_or(self.line_idx))?;
        let (anchor_side, _) = Self::comment_target_for_line(anchor_line)?;
        let mut start_line = None;
        let mut end_line = None;
        for line in &file.lines[start_idx..=end_idx] {
            let Some((side, lineno)) = Self::comment_target_for_line(line) else {
                continue;
            };
            if side != anchor_side {
                continue;
            }
            if start_line.is_none() {
                start_line = Some(lineno);
            }
            end_line = Some(lineno);
        }
        let end_line = end_line?;
        let start_line_value = start_line?;
        let range_start = if start_line_value == end_line { None } else { Some(start_line_value) };
        let range_side = if range_start.is_some() { Some(anchor_side) } else { None };
        Some((range_side, range_start, anchor_side, end_line, file.path.clone()))
    }

    fn comment_target_for_line(line: &crate::diff::DiffLine) -> Option<(Side, u32)> {
        match line.kind {
            LineKind::Del => line.old_lineno.map(|ln| (Side::Left, ln)),
            LineKind::HunkHeader => None,
            _ => line.new_lineno.or(line.old_lineno).map(|ln| (Side::Right, ln)),
        }
    }

    fn comment_contains_line(comment: &Comment, side: Side, line: u32) -> bool {
        if comment.side != side {
            return false;
        }
        let start = comment.start_line.unwrap_or(comment.line);
        line >= start && line <= comment.line
    }

    fn comment_ends_on_line(comment: &Comment, line: &crate::diff::DiffLine) -> bool {
        match comment.side {
            Side::Right => line.new_lineno == Some(comment.line) && line.kind != LineKind::Del,
            Side::Left => line.old_lineno == Some(comment.line) && line.kind != LineKind::Add,
        }
    }

    fn comment_covers_line(comment: &Comment, line: &crate::diff::DiffLine) -> bool {
        let current = match comment.side {
            Side::Right => {
                if line.kind == LineKind::Del {
                    return false;
                }
                line.new_lineno
            }
            Side::Left => {
                if line.kind == LineKind::Add {
                    return false;
                }
                line.old_lineno
            }
        };
        let Some(current) = current else {
            return false;
        };
        let start = comment.start_line.unwrap_or(comment.line);
        current >= start && current <= comment.line
    }

    fn ensure_file_overlay_visible(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.file_overlay_idx < self.file_overlay_scroll {
            self.file_overlay_scroll = self.file_overlay_idx;
        } else if self.file_overlay_idx >= self.file_overlay_scroll + height {
            self.file_overlay_scroll = self.file_overlay_idx + 1 - height;
        }
    }

    fn ensure_comment_overlay_visible(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.comment_overlay_idx < self.comment_overlay_scroll {
            self.comment_overlay_scroll = self.comment_overlay_idx;
        } else if self.comment_overlay_idx >= self.comment_overlay_scroll + height {
            self.comment_overlay_scroll = self.comment_overlay_idx + 1 - height;
        }
    }

    pub fn pet_snappy(&mut self) {
        let msg = self.snappy.pet();
        self.status = format!("snappy: {msg}");
    }

    pub fn execute_command(&mut self, cmd_raw: &str) -> Result<bool> {
        let cmd = cmd_raw.trim();
        let cmd = cmd.strip_prefix(':').unwrap_or(cmd);
        let (verb, arg) = match cmd.split_once(char::is_whitespace) {
            Some((v, a)) => (v, a.trim()),
            None => (cmd, ""),
        };
        match verb {
            "" => Ok(false),
            "q" | "quit" | "exit" => Ok(true),
            "h" | "help" => {
                self.open_help();
                Ok(false)
            }
            "c" | "comment" => {
                self.begin_comment();
                Ok(false)
            }
            "d" | "del" | "delete" => {
                self.delete_current_comment();
                Ok(false)
            }
            "files" | "f" => {
                self.toggle_files_overlay();
                Ok(false)
            }
            "comments" | "drafts" | "m" => {
                self.toggle_comments_overlay();
                Ok(false)
            }
            "w" | "save" => {
                comments::save(&self.repo_root, &self.store)?;
                self.status = "comments saved".into();
                Ok(false)
            }
            "r" | "reload" => {
                self.reload_diff();
                Ok(false)
            }
            "p" | "publish" => {
                self.publish_all();
                Ok(false)
            }
            "pr" | "pcreate" => {
                let title_override = if arg.is_empty() { None } else { Some(arg.to_string()) };
                self.create_pr(title_override);
                Ok(false)
            }
            "range" => {
                self.toggle_review_range();
                Ok(false)
            }
            "v" | "voice" => {
                self.toggle_voice();
                Ok(false)
            }
            "pet" => {
                self.pet_snappy();
                Ok(false)
            }
            other => {
                self.status = format!("unknown command: :{other}  ·  try :help");
                let _ = arg;
                Ok(false)
            }
        }
    }

    pub fn toggle_voice(&mut self) {
        // already transcribing — ignore toggle
        if self.transcribe_rx.is_some() {
            self.status = "still transcribing…".into();
            return;
        }
        if let Some(rec) = self.recorder.take() {
            self.recording_since = None;
            match rec.stop() {
                Ok(wav) => {
                    let (tx, rx) = mpsc::channel();
                    thread::spawn(move || {
                        let _ = tx.send(voice::transcribe(&wav));
                    });
                    self.transcribe_rx = Some(rx);
                    self.transcribe_started = Some(Instant::now());
                    self.status = "⠋ transcribing…".into();
                }
                Err(e) => self.status = format!("recording failed: {e}"),
            }
        } else {
            let wav = self.repo_root.join(".revu").join("recording.wav");
            let _ = comments::ensure_dir(&self.repo_root);
            match voice::Recorder::start(wav) {
                Ok(r) => {
                    self.recorder = Some(r);
                    self.recording_since = Some(Instant::now());
                    self.status = "🎙  recording 0.0s  ·  :v to stop".into();
                }
                Err(e) => self.status = format!("mic error: {e}"),
            }
        }
    }

    fn poll_voice(&mut self) -> bool {
        let mut changed = false;
        // live elapsed timer while recording
        if let Some(since) = self.recording_since {
            let secs = since.elapsed().as_secs_f32();
            let stop_hint = if matches!(self.mode, Mode::Comment(_)) {
                "Ctrl+V to stop"
            } else {
                ":v to stop"
            };
            self.status = format!("🎙  recording {:.1}s  ·  {}", secs, stop_hint);
            changed = true;
        }
        // spinner + timer while transcribing, and handoff when done
        if self.transcribe_rx.is_some() {
            let secs = self
                .transcribe_started
                .map(|t| t.elapsed().as_secs_f32())
                .unwrap_or(0.0);
            let frames = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];
            let idx = (secs * 10.0) as usize % frames.len();
            self.status = format!("{} transcribing… {:.1}s", frames[idx], secs);
            changed = true;

            let result = self.transcribe_rx.as_ref().map(|rx| rx.try_recv());
            match result {
                Some(Ok(res)) => {
                    self.transcribe_rx = None;
                    let elapsed = secs;
                    match res {
                        Ok(text) => {
                            let text = text.trim().to_string();
                            if !matches!(self.mode, Mode::Comment(_)) {
                                self.begin_comment();
                            }
                            if !matches!(self.mode, Mode::Comment(_)) {
                                self.seek_to_commentable_line();
                                self.begin_comment();
                            }
                            if !matches!(self.mode, Mode::Comment(_)) {
                                // No commentable line available — still open an
                                // editable scratch draft so the user can review
                                // and copy the text. Saving will be a no-op.
                                self.mode = Mode::Comment(String::new());
                                self.comment_cursor = 0;
                                self.status =
                                    "no commentable line — transcript shown in scratch draft".into();
                            }
                            if let Mode::Comment(draft) = &mut self.mode {
                                if !draft.is_empty() && !draft.ends_with(' ') {
                                    draft.push(' ');
                                }
                                draft.push_str(&text);
                                self.comment_cursor = draft.chars().count();
                            }
                            let preview: String = text.chars().take(60).collect();
                            let ellipsis = if text.chars().count() > 60 { "…" } else { "" };
                            if !self.status.starts_with("no commentable") {
                                self.status =
                                    format!("🎙  {:.1}s  “{}{}”", elapsed, preview, ellipsis);
                            }
                        }
                        Err(e) => self.status = format!("transcribe failed: {e}"),
                    }
                    self.transcribe_started = None;
                }
                Some(Err(TryRecvError::Disconnected)) => {
                    self.transcribe_rx = None;
                    self.transcribe_started = None;
                    self.status = "transcribe worker exited unexpectedly".into();
                }
                _ => {}
            }
        }
        changed
    }

    fn publish_all(&mut self) {
        if !publish::gh_available() {
            self.status = "`gh` CLI not found — install from https://cli.github.com".into();
            return;
        }
        let pr = match publish::current_pr_number(&self.repo_root) {
            Some(n) => n,
            None => {
                self.status = "no PR for this branch. run :pcreate to create one".into();
                return;
            }
        };
        let to_send: Vec<usize> = self
            .store
            .comments
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.published && !c.orphaned && !c.body.is_empty())
            .map(|(i, _)| i)
            .collect();
        if to_send.is_empty() {
            self.status = "nothing to publish".into();
            return;
        }
        let mut ok = 0usize;
        let mut fail = 0usize;
        let mut last_err = String::new();
        for i in to_send {
            let c = self.store.comments[i].clone();
            match publish::post_review_comment(&self.repo_root, pr, &c) {
                Ok(_) => {
                    self.store.comments[i].published = true;
                    ok += 1;
                }
                Err(e) => {
                    fail += 1;
                    last_err = format!("{e}");
                }
            }
        }
        let _ = comments::save(&self.repo_root, &self.store);
        if fail == 0 {
            self.status = format!("published {ok} comment(s) to PR #{pr}");
        } else {
            self.status = format!("published {ok}, failed {fail}: {last_err}");
        }
    }

    fn create_pr(&mut self, title_override: Option<String>) {
        if !publish::gh_available() {
            self.status = "`gh` CLI not found — install from https://cli.github.com".into();
            return;
        }
        self.status = "creating PR...".into();
        match publish::create_pr(&self.repo_root, title_override.as_deref()) {
            Ok(n) => {
                self.status = format!("created PR #{n}");
            }
            Err(e) => {
                self.status = format!("failed to create PR: {e}");
            }
        }
    }
}

fn count_changes(file: &FileDiff) -> (usize, usize) {
    let mut adds = 0;
    let mut dels = 0;
    for line in &file.lines {
        match line.kind {
            LineKind::Add => adds += 1,
            LineKind::Del => dels += 1,
            _ => {}
        }
    }
    (adds, dels)
}
