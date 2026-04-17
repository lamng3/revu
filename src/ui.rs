use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use std::collections::BTreeSet;

use crate::app::{App, Mode, Overlay};
use crate::comments::Side;
use crate::diff::{DiffLine, LineKind};

const BG: Color = Color::Black;
const PANEL: Color = Color::Black;
const BORDER: Color = Color::DarkGray;
const TEXT: Color = Color::White;
const MUTED: Color = Color::Gray;
const ADD_FG: Color = Color::Green;
const ADD_BG: Color = Color::Indexed(22);
const DEL_FG: Color = Color::Red;
const DEL_BG: Color = Color::Indexed(52);
const HUNK_FG: Color = Color::Cyan;
const HUNK_BG: Color = Color::Black;
const SEL_BG: Color = Color::DarkGray;
const RANGE_BG: Color = Color::Indexed(18);      // deep indigo — stands out on black
const RANGE_EDGE: Color = Color::LightYellow;    // bright ▌ along the left edge
const RANGE_FG: Color = Color::LightCyan;        // line-number + text accent inside the range
const COMMENT_DRAFT: Color = Color::Yellow;
const COMMENT_PUBLISHED: Color = Color::Green;
const COMMENT_ORPHAN: Color = Color::Yellow;
const SNAPPY_WATER: Color = Color::Rgb(82, 175, 162);
const SNAPPY_DEEP: Color = Color::Rgb(52, 124, 116);
const SNAPPY_BUBBLE: Color = Color::Rgb(196, 232, 222);
const SNAPPY_LOBSTER: Color = Color::Rgb(232, 88, 72);
const SNAPPY_LOBSTER_HI: Color = Color::Rgb(255, 150, 120);

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BG)), size);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(8), Constraint::Length(3)])
        .split(size);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(40), Constraint::Min(40)])
        .split(layout[1]);

    app.click.file_rows.clear();
    app.click.folder_rows.clear();
    app.click.comment_rows.clear();
    app.click.diff_rows.clear();
    app.click.file_panel_bounds = None;
    app.click.diff_panel_bounds = None;

    draw_banner(f, layout[0], app);
    draw_files_panel(f, body[0], app);
    draw_diff(f, body[1], app);
    draw_footer(f, layout[2], app);

    match app.overlay {
        Overlay::Help => draw_help_overlay(f, size, app),
        Overlay::Files => draw_files_overlay(f, size, app),
        Overlay::Comments => draw_comments_overlay(f, size, app),
        Overlay::None => {}
    }

    if let Mode::Comment(draft) = &app.mode {
        draw_comment_editor(f, size, app, draft);
    }
}

fn draw_comment_editor(f: &mut Frame, area: Rect, app: &App, draft: &str) {
    let transcribing = app.transcribe_rx.is_some();
    let recording = app.recorder.is_some();
    let (anchor_file, anchor_line) = app
        .current_file()
        .and_then(|file| {
            file.lines.get(app.line_idx).map(|l| {
                let ln = l.new_lineno.or(l.old_lineno).unwrap_or(0);
                (file.path.clone(), ln)
            })
        })
        .unwrap_or_else(|| ("(no file)".to_string(), 0));

    let width = area.width.saturating_sub(8).min(96).max(40);
    let height = area.height.saturating_sub(4).min(18).max(8);
    let rect = centered_rect(area, width, height);
    f.render_widget(Clear, rect);

    let title = if recording {
        let secs = app
            .recording_since
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        let blink = (secs * 2.0) as u32 % 2 == 0;
        let dot = if blink { "●" } else { "○" };
        format!(" comment · {anchor_file}:{anchor_line} · {dot} REC {secs:>4.1}s  Ctrl+V to stop ")
    } else if transcribing {
        let secs = app
            .transcribe_started
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let idx = (secs * 10.0) as usize % frames.len();
        format!(
            " comment · {anchor_file}:{anchor_line} · {} transcribing… {secs:.1}s ",
            frames[idx]
        )
    } else {
        format!(" comment · {anchor_file}:{anchor_line} ")
    };

    let border_color = if recording || transcribing {
        Color::LightRed
    } else {
        COMMENT_DRAFT
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(PANEL))
        .title(Span::styled(title, Style::default().fg(border_color).add_modifier(Modifier::BOLD)));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    // split body and footer
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(2), Constraint::Length(1)])
        .split(inner);

    let chars = draft.chars().count();
    let words = draft.split_whitespace().count();
    let cursor = app.comment_cursor.min(chars);

    let body_style = if draft.is_empty() && !transcribing {
        Style::default().fg(MUTED).bg(PANEL)
    } else {
        Style::default().fg(TEXT).bg(PANEL)
    };

    let body_paragraph: Paragraph = if draft.is_empty() && !transcribing {
        Paragraph::new("(start typing, or :v to dictate)".to_string())
    } else {
        let split_byte = draft
            .char_indices()
            .nth(cursor)
            .map(|(b, _)| b)
            .unwrap_or(draft.len());
        let before = &draft[..split_byte];
        let after = &draft[split_byte..];
        let caret_style = Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD);
        let text_style = Style::default().fg(TEXT);
        let mut lines: Vec<Line> = Vec::new();
        let mut current: Vec<Span> = Vec::new();
        for (i, segment) in before.split('\n').enumerate() {
            if i > 0 {
                lines.push(Line::from(std::mem::take(&mut current)));
            }
            current.push(Span::styled(segment.to_string(), text_style));
        }
        current.push(Span::styled("▎".to_string(), caret_style));
        for (i, segment) in after.split('\n').enumerate() {
            if i > 0 {
                lines.push(Line::from(std::mem::take(&mut current)));
            }
            current.push(Span::styled(segment.to_string(), text_style));
        }
        lines.push(Line::from(current));
        Paragraph::new(lines)
    };

    f.render_widget(
        body_paragraph
            .wrap(Wrap { trim: false })
            .style(body_style),
        parts[0],
    );

    let key = Style::default().fg(COMMENT_DRAFT).add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(MUTED);
    let footer = Line::from(vec![
        Span::styled("← → ↑ ↓", key),
        Span::styled(" move   ", muted),
        Span::styled("Enter", key),
        Span::styled(" save   ", muted),
        Span::styled("Ctrl+J", key),
        Span::styled(" newline   ", muted),
        Span::styled("Ctrl+V", key),
        Span::styled(" dictate more   ", muted),
        Span::styled("Esc", key),
        Span::styled(" cancel   ", muted),
        Span::styled(format!("  {chars}c · {words}w"), muted),
    ]);
    f.render_widget(
        Paragraph::new(footer).style(Style::default().bg(PANEL)),
        parts[1],
    );
}

fn draw_banner(f: &mut Frame, area: Rect, app: &App) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    let info_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG))
        .title(Span::styled(" revu ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)));
    let snappy_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightRed))
        .title(Span::styled(" 🦞 snappy ", Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD)));

    let info_inner = info_block.inner(sections[0]);
    let snappy_inner = snappy_block.inner(sections[1]);
    f.render_widget(info_block, sections[0]);
    f.render_widget(snappy_block, sections[1]);

    let voice = if app.recorder.is_some() { "REC" } else { "idle" };
    let snappy_message = app
        .snappy
        .current_message
        .clone()
        .unwrap_or_else(|| "Snappy: you got this.".to_string());
    let snappy_message = snappy_message
        .strip_prefix("Snappy: ")
        .unwrap_or(&snappy_message)
        .to_string();
    let info = Line::from(vec![
        Span::styled("repo ", Style::default().fg(MUTED)),
        Span::styled(app.repo_name(), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("branch ", Style::default().fg(MUTED)),
        Span::styled(app.branch_name.clone(), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(format!("drafts {}", app.unpublished_count()), Style::default().fg(COMMENT_DRAFT)),
        Span::raw("  "),
        Span::styled(format!("voice {}", voice), Style::default().fg(MUTED)),
    ]);
    let current = Line::from(vec![
        Span::styled("base ", Style::default().fg(MUTED)),
        Span::styled(app.base_label(), Style::default().fg(TEXT)),
        Span::raw("   "),
        Span::styled(":help", Style::default().fg(MUTED)),
    ]);
    let _ = SNAPPY_BUBBLE;
    let _ = SNAPPY_DEEP;
    let hint = Line::from("");
    let stage_width = snappy_inner.width as usize;
    let frame = app.snappy.duck_frame;

    // Claws + eyes animate from frame (cycle ~2.2s).
    let claw_phase = frame % 16;
    let (l_claw, r_claw) = match claw_phase {
        0..=11 => ("<(", ")>"),
        12..=13 => ("<<", ">>"),
        14 => ("«(", ")»"),
        _ => ("<(", ")>"),
    };
    let snapping = claw_phase == 14;
    let eyes = if frame % 90 < 2 { "°‿°" } else { "°º°" };

    // Water flowing past stationary lobster.
    let wave_seed: Vec<char> = "~∿~≈~∿≈~∿~≈~∿≈~∿~≈~∿≈~∿~≈~∿≈~∿~≈~∿≈~∿~≈~∿≈".chars().collect();
    let ws = wave_seed.len().max(1);
    let lobster_body: String = format!("{l_claw}{eyes}{r_claw}");
    let lobster_w = lobster_body.chars().count();
    let center = stage_width.saturating_sub(lobster_w) / 2;
    let shift = frame % ws;

    let left_water: String = (0..center)
        .map(|i| wave_seed[(i + shift) % ws])
        .collect();
    let right_start = center + lobster_w;
    let right_w = stage_width.saturating_sub(right_start);
    let right_water: String = (0..right_w)
        .map(|i| wave_seed[(i + right_start + shift) % ws])
        .collect();

    let water_style = Style::default().fg(Color::Cyan);
    let lobster_color = if snapping { Color::LightRed } else { Color::Red };
    let lobster_style = Style::default().fg(lobster_color).add_modifier(Modifier::BOLD);

    let scene = Line::from(vec![
        Span::styled(left_water, water_style),
        Span::styled(lobster_body, lobster_style),
        Span::styled(right_water, water_style),
    ]);

    let message_line = Line::from(vec![
        Span::styled("» Snappy: ", Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD)),
        Span::styled(snappy_message, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
    ]);

    f.render_widget(
        Paragraph::new(vec![info, current])
            .style(Style::default().bg(BG)),
        info_inner,
    );
    f.render_widget(
        Paragraph::new(vec![scene, message_line]),
        snappy_inner,
    );
    let _ = hint;
    let _ = SNAPPY_WATER;
    let _ = SNAPPY_LOBSTER;
    let _ = SNAPPY_LOBSTER_HI;
}

fn draw_files_panel(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG))
        .title(Span::styled(" files ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.click.file_panel_bounds = Some((inner.x, inner.y, inner.width, inner.height));

    if app.files.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled("no diff files", Style::default().fg(MUTED))))
                .style(Style::default().bg(BG)),
            inner,
        );
        return;
    }

    let rows = build_file_tree_rows(app);
    let visible = inner.height as usize;
    app.file_panel_viewport_height = visible;
    if app.follow_selected_file {
        let selected_row = rows
            .iter()
            .position(|row| row.file_idx == Some(app.file_idx))
            .unwrap_or(0);
        app.ensure_file_panel_visible_for_row(selected_row, rows.len());
        app.follow_selected_file = false;
    } else {
        let max_scroll = rows.len().saturating_sub(visible);
        app.file_panel_scroll = app.file_panel_scroll.min(max_scroll);
    }
    let start = app.file_panel_scroll.min(rows.len().saturating_sub(1));
    let end = (start + visible).min(rows.len());
    let mut lines = Vec::new();
    for (row_idx, row) in rows[start..end].iter().enumerate() {
        let style = if row.file_idx == Some(app.file_idx) {
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD).bg(SEL_BG)
        } else if row.is_folder {
            Style::default().fg(MUTED).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(MUTED)
        };
        let mut spans = vec![
            Span::styled(" ".repeat(row.indent * 2), Style::default()),
            Span::styled(row.glyph.to_string(), style),
            Span::raw(" "),
            Span::styled(row.label.clone(), style),
        ];
        if let Some((adds, dels)) = row.counts {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(format!("+{adds}"), Style::default().fg(ADD_FG)));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(format!("-{dels}"), Style::default().fg(DEL_FG)));
        }
        lines.push(Line::from(spans));
        let screen_row = inner.y + row_idx as u16;
        if let Some(folder_key) = &row.folder_key {
            app.click.folder_rows.push((screen_row, screen_row + 1, folder_key.clone()));
        } else if let Some(file_idx) = row.file_idx {
            app.click.file_rows.push((screen_row, screen_row + 1, file_idx));
        }
    }

    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(BG)),
        inner,
    );
}

fn draw_diff(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().bg(PANEL))
        .border_style(Style::default().fg(BORDER))
        .title(Span::styled(" review ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.click.diff_panel_bounds = Some((inner.x, inner.y, inner.width, inner.height));

    let Some(file) = app.current_file().cloned() else {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled("no diff to show", Style::default().fg(MUTED))))
                .style(Style::default().bg(PANEL)),
            inner,
        );
        return;
    };

    let inline_editor = matches!(app.mode, Mode::Comment(_)) && inner.height > 2;
    let header_h = 1u16;
    let editor_h = if inline_editor { 2u16 } else { 0u16 };
    let diff_h = inner.height.saturating_sub(header_h + editor_h);
    app.diff_viewport_height = diff_h;

    let (adds, dels) = app.file_change_counts(app.file_idx);
    let mut header_spans = vec![
        Span::styled(file.path.clone(), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(format!("+{adds}"), Style::default().fg(ADD_FG)),
        Span::raw(" "),
        Span::styled(format!("-{dels}"), Style::default().fg(DEL_FG)),
        Span::raw("  "),
        Span::styled(
            format!("line {}/{}", app.line_idx.saturating_add(1), file.lines.len().max(1)),
            Style::default().fg(MUTED),
        ),
    ];
    if app.multiline_active() {
        if let Some((start, end)) = app.multiline_range() {
            header_spans.push(Span::raw("  "));
            header_spans.push(Span::styled(
                format!("▌ range {}–{} ({} lines)", start, end, end - start + 1),
                Style::default().fg(RANGE_EDGE).add_modifier(Modifier::BOLD),
            ));
        }
    }
    let header = Line::from(header_spans);
    f.render_widget(
        Paragraph::new(header).style(Style::default().bg(PANEL)),
        Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
    );

    let start = app.scroll;
    let mut idx = start;
    let mut row_offset = 0u16;
    let mut selected_visible = false;

    while idx < file.lines.len() && row_offset < diff_h {
        let draw_y = inner.y + 1 + row_offset;
        let line = &file.lines[idx];
        app.click.diff_rows.push((draw_y, idx));
        draw_diff_row(
            f,
            Rect { x: inner.x, y: draw_y, width: inner.width, height: 1 },
            app,
            line,
            idx,
            idx == app.line_idx,
        );
        if idx == app.line_idx {
            selected_visible = true;
        }
        row_offset += 1;
        if let Some(body) = app.comment_body_for_line(line) {
            if row_offset < diff_h {
                let bubble_y = inner.y + 1 + row_offset;
                draw_comment_row(
                    f,
                    Rect { x: inner.x, y: bubble_y, width: inner.width, height: 1 },
                    body,
                    app.comment_marker_for_line(line),
                    idx == app.line_idx,
                );
                // Register click target for the inline bubble so clicking it
                // opens the editor on the anchor diff line above.
                app.click.comment_rows.push((bubble_y, bubble_y + 1, idx));
                row_offset += 1;
            }
        }
        idx += 1;
    }

    if inline_editor && selected_visible {
        draw_inline_comment_editor(
            f,
            Rect {
                x: inner.x,
                y: inner.y + inner.height.saturating_sub(2),
                width: inner.width,
                height: 2,
            },
            app,
        );
    }
}

pub fn file_tree_row_count(app: &App) -> usize {
    build_file_tree_rows(app).len()
}

fn draw_diff_row(f: &mut Frame, area: Rect, app: &App, line: &DiffLine, idx: usize, selected: bool) {
    let (fg, bg, marker) = style_for(line);
    let marker_style = comment_marker_style(app.comment_marker_for_line(line));
    let old_n = line.old_lineno.map(|n| format!("{n:>4}")).unwrap_or_else(|| "    ".to_string());
    let new_n = line.new_lineno.map(|n| format!("{n:>4}")).unwrap_or_else(|| "    ".to_string());
    let in_range = app.line_in_selected_range(idx);
    let row_bg = if selected {
        SEL_BG
    } else if in_range {
        RANGE_BG
    } else {
        bg.unwrap_or(PANEL)
    };
    let range_edge = if in_range { '█' } else { ' ' };
    let edge_style = if in_range {
        Style::default().fg(RANGE_EDGE).bg(row_bg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(MUTED).bg(row_bg)
    };
    let number_style = if selected {
        Style::default().fg(TEXT).bg(row_bg).add_modifier(Modifier::BOLD)
    } else if in_range {
        Style::default().fg(RANGE_FG).bg(row_bg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(MUTED).bg(row_bg)
    };
    let text_style = if selected {
        Style::default().fg(fg).bg(row_bg).add_modifier(Modifier::BOLD)
    } else if in_range {
        Style::default().fg(fg).bg(row_bg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(fg).bg(row_bg)
    };

    let spans = vec![
        Span::styled(format!("{range_edge}"), edge_style),
        Span::styled(format!(" {old_n} {new_n} "), number_style),
        Span::styled(
            format!(" {} ", app.comment_marker_for_line(line).unwrap_or(' ')),
            marker_style.bg(row_bg),
        ),
        Span::styled(format!(" {marker} "), text_style),
        Span::styled(line.text.clone(), text_style),
    ];
    f.render_widget(Paragraph::new(Line::from(spans)).style(Style::default().bg(row_bg)), area);
}

fn draw_comment_row(
    f: &mut Frame,
    area: Rect,
    body: String,
    marker: Option<char>,
    selected_line: bool,
) {
    let bg = if selected_line { SEL_BG } else { PANEL };
    let label = match marker {
        Some('P') => "published",
        Some('!') => "orphaned",
        _ => "draft",
    };
    let style = match marker {
        Some('P') => Style::default().fg(COMMENT_PUBLISHED).bg(bg),
        Some('!') => Style::default().fg(COMMENT_ORPHAN).bg(bg),
        _ => Style::default().fg(COMMENT_DRAFT).bg(bg),
    };
    let preview: String = body.chars().take(area.width.saturating_sub(22) as usize).collect();
    let line = Line::from(vec![
        Span::styled("         review ", style.add_modifier(Modifier::BOLD)),
        Span::styled(format!("[{label}] "), style),
        Span::styled(preview, Style::default().fg(TEXT).bg(bg)),
    ]);
    f.render_widget(Paragraph::new(line).style(Style::default().bg(bg)), area);
}

fn draw_inline_comment_editor(f: &mut Frame, area: Rect, app: &App) {
    let draft = match &app.mode {
        Mode::Comment(draft) => draft.clone(),
        _ => return,
    };
    let preview = draft.replace('\n', " \\ ");
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(PANEL));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let range_hint = if app.review_range_anchor.is_some() {
        "  V range  c comment  x delete"
    } else {
        "  V start range"
    };
    let lines = vec![
        Line::from(vec![
            Span::styled("comment ", Style::default().fg(COMMENT_DRAFT).add_modifier(Modifier::BOLD)),
            Span::styled("enter save", Style::default().fg(MUTED)),
            Span::raw("  "),
            Span::styled("esc cancel", Style::default().fg(MUTED)),
            Span::raw("  "),
            Span::styled("shift+enter newline", Style::default().fg(MUTED)),
            Span::styled(range_hint, Style::default().fg(MUTED)),
        ]),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(COMMENT_DRAFT).add_modifier(Modifier::BOLD)),
            Span::styled(preview, Style::default().fg(TEXT)),
            Span::styled("▎", Style::default().fg(COMMENT_DRAFT)),
        ]),
    ];
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).style(Style::default().bg(PANEL)),
        inner,
    );
}

fn comment_marker_style(marker: Option<char>) -> Style {
    match marker {
        Some('D') => Style::default().fg(COMMENT_DRAFT).add_modifier(Modifier::BOLD),
        Some('P') => Style::default().fg(COMMENT_PUBLISHED).add_modifier(Modifier::BOLD),
        Some('!') => Style::default().fg(COMMENT_ORPHAN).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(MUTED),
    }
}

fn style_for(line: &DiffLine) -> (Color, Option<Color>, &'static str) {
    match line.kind {
        LineKind::Add => (ADD_FG, Some(ADD_BG), "+"),
        LineKind::Del => (DEL_FG, Some(DEL_BG), "-"),
        LineKind::Context => (TEXT, None, " "),
        LineKind::HunkHeader => (HUNK_FG, Some(HUNK_BG), "@"),
    }
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let line = match &app.mode {
        Mode::Command(buf) => Line::from(vec![
            Span::styled(":", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(buf.clone(), Style::default().fg(TEXT)),
            Span::styled("▎", Style::default().fg(TEXT)),
            Span::raw("  "),
            Span::styled("enter run", Style::default().fg(MUTED)),
            Span::raw("  "),
            Span::styled("esc cancel", Style::default().fg(MUTED)),
        ]),
        Mode::Comment(draft) => {
            let preview: String = draft.replace('\n', " \\ ").chars().take(inner.width.saturating_sub(32) as usize).collect();
            Line::from(vec![
                Span::styled("review comment ", Style::default().fg(COMMENT_DRAFT).add_modifier(Modifier::BOLD)),
                Span::styled(preview, Style::default().fg(TEXT)),
                Span::raw("  "),
                Span::styled("enter save", Style::default().fg(MUTED)),
                Span::raw("  "),
                Span::styled("esc cancel", Style::default().fg(MUTED)),
            ])
        }
        Mode::Normal => Line::from(vec![
            Span::styled(app.status.clone(), Style::default().fg(TEXT)),
            Span::raw("  ·  "),
            Span::styled("press :help for commands", Style::default().fg(MUTED)),
        ]),
    };
    f.render_widget(
        Paragraph::new(line)
            .wrap(Wrap { trim: true })
            .style(Style::default().bg(BG)),
        inner,
    );
}

#[derive(Clone)]
struct FileTreeRow {
    indent: usize,
    glyph: char,
    label: String,
    counts: Option<(usize, usize)>,
    file_idx: Option<usize>,
    folder_key: Option<String>,
    is_folder: bool,
}

fn build_file_tree_rows(app: &App) -> Vec<FileTreeRow> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for (idx, file) in app.files.iter().enumerate() {
        let parts: Vec<&str> = file.path.split('/').collect();
        let mut hidden_by_collapsed_parent = false;
        if parts.len() > 1 {
            for depth in 0..parts.len() - 1 {
                let key = parts[..=depth].join("/");
                if depth > 0 {
                    let parent = parts[..depth].join("/");
                    if app.collapsed_folders.contains(&parent) {
                        hidden_by_collapsed_parent = true;
                    }
                }
                if seen.insert(key.clone()) {
                    if !hidden_by_collapsed_parent {
                        let collapsed = app.collapsed_folders.contains(&key);
                        rows.push(FileTreeRow {
                            indent: depth,
                            glyph: if collapsed { '▸' } else { '▾' },
                            label: parts[depth].to_string(),
                            counts: None,
                            file_idx: None,
                            folder_key: Some(key.clone()),
                            is_folder: true,
                        });
                    }
                }
            }
        }
        let parent_collapsed = if parts.len() > 1 {
            (1..parts.len()).any(|depth| app.collapsed_folders.contains(&parts[..depth].join("/")))
        } else {
            false
        };
        if !parent_collapsed {
            rows.push(FileTreeRow {
                indent: parts.len().saturating_sub(1),
                glyph: if idx == app.file_idx { '›' } else { '•' },
                label: parts.last().copied().unwrap_or(&file.path).to_string(),
                counts: Some(app.file_change_counts(idx)),
                file_idx: Some(idx),
                folder_key: None,
                is_folder: false,
            });
        }
    }
    rows
}

fn draw_files_overlay(f: &mut Frame, area: Rect, app: &mut App) {
    let width = area.width.min(84);
    let height = area.height.min(24);
    let rect = centered_rect(area, width, height);
    f.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(PANEL))
        .title(Span::styled(" files ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let visible = inner.height as usize;
    let start = app.file_overlay_scroll.min(app.files.len().saturating_sub(1));
    let end = (start + visible).min(app.files.len());
    let mut lines = Vec::new();
    for idx in start..end {
        let file = &app.files[idx];
        let (adds, dels) = app.file_change_counts(idx);
        let selected = idx == app.file_overlay_idx;
        let style = if selected { Style::default().fg(TEXT).add_modifier(Modifier::BOLD) } else { Style::default().fg(MUTED) };
        lines.push(Line::from(vec![
            Span::styled(if selected { "> " } else { "  " }, style),
            Span::styled(file.path.clone(), style),
            Span::raw("  "),
            Span::styled(format!("+{adds}"), Style::default().fg(ADD_FG)),
            Span::raw(" "),
            Span::styled(format!("-{dels}"), Style::default().fg(DEL_FG)),
        ]));
        let row = inner.y + (idx - start) as u16;
        app.click.file_rows.push((row, row + 1, idx));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("no files in the current diff", Style::default().fg(MUTED))));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).style(Style::default().bg(PANEL)), inner);
}

fn draw_comments_overlay(f: &mut Frame, area: Rect, app: &mut App) {
    // Drop any inline bubble click targets — this overlay fully occludes the
    // diff, and its own rows will be registered below.
    app.click.comment_rows.clear();
    let width = area.width.min(92);
    let height = area.height.min(24);
    let rect = centered_rect(area, width, height);
    f.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(PANEL))
        .title(Span::styled(" comments ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let visible_comments = (inner.height as usize / 2).max(1);
    let start = app.comment_overlay_scroll.min(app.store.comments.len().saturating_sub(1));
    let end = (start + visible_comments).min(app.store.comments.len());
    let mut lines = Vec::new();
    for idx in start..end {
        let comment = &app.store.comments[idx];
        let selected = idx == app.comment_overlay_idx;
        let pointer_style = if selected { Style::default().fg(TEXT).add_modifier(Modifier::BOLD) } else { Style::default().fg(MUTED) };
        let icon_style = if comment.orphaned {
            Style::default().fg(COMMENT_ORPHAN)
        } else if comment.published {
            Style::default().fg(COMMENT_PUBLISHED)
        } else {
            Style::default().fg(COMMENT_DRAFT)
        };
        let side = match comment.side { Side::Left => "LEFT", Side::Right => "RIGHT" };
        let preview: String = comment.body.chars().take(70).collect();
        let lines_span = match comment.start_line {
            Some(start) if start != comment.line => {
                format!("L{start}–L{} ({} lines)", comment.line, comment.line.saturating_sub(start) + 1)
            }
            _ => format!("L{}", comment.line),
        };
        let state = if comment.orphaned {
            " · orphaned"
        } else if comment.published {
            " · published"
        } else {
            " · draft"
        };
        lines.push(Line::from(vec![
            Span::styled(if selected { "> " } else { "  " }, pointer_style),
            Span::styled("* ", icon_style),
            Span::styled(comment.file.clone(), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {lines_span}  {side}"), Style::default().fg(TEXT)),
            Span::styled(state.to_string(), Style::default().fg(MUTED)),
        ]));
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(preview, Style::default().fg(MUTED)),
        ]));
        let row_top = inner.y + ((idx - start) as u16) * 2;
        app.click.comment_rows.push((row_top, row_top + 2, idx));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("no saved comments yet. press c on a diff line.", Style::default().fg(MUTED))));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).style(Style::default().bg(PANEL)), inner);
}

fn draw_help_overlay(f: &mut Frame, area: Rect, app: &App) {
    let key_style = Style::default().fg(COMMENT_DRAFT).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(TEXT);
    let muted_style = Style::default().fg(MUTED);
    let heading_style = Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD);
    let accent_style = Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD);

    let row = |key: &str, desc: &str| -> Line {
        Line::from(vec![
            Span::styled(format!("  {:<14}", key), key_style),
            Span::styled(desc.to_string(), text_style),
        ])
    };
    let heading = |label: &str| -> Line {
        Line::from(Span::styled(label.to_string(), heading_style))
    };

    let lines = vec![
        heading("  NAVIGATE"),
        row("j / k", "move by line"),
        row("Tab / ⇧Tab", "next / previous file"),
        row("n / N", "next / previous hunk"),
        row("} / {", "next / previous commented line"),
        row("g / G", "top / bottom of file"),
        row("J", "jump to latest diff"),
        Line::from(""),
        heading("  COMMENT"),
        row("c", "add or edit comment on this line"),
        row("V", "start / clear multi-line range"),
        row("x", "delete draft on this line"),
        row("m", "open saved comments"),
        Line::from(""),
        heading("  EDITOR"),
        row("type", "insert text at the ▎ caret"),
        row("← / →", "move caret by one character"),
        row("↑ / ↓", "move caret between lines"),
        row("Home / End", "jump to line start / end"),
        row("Backspace", "delete before caret"),
        row("Delete", "delete at caret"),
        row("Ctrl+U", "clear the whole draft"),
        row("Ctrl+V", "dictate more — appends voice to the draft"),
        row("Enter", "save draft"),
        row("Ctrl+J", "newline at caret (⌥Enter also works)"),
        row("Esc", "cancel without saving"),
        Line::from(""),
        heading("  COMMANDS"),
        row(":p", "publish drafts to your PR"),
        row(":pr", "create a PR if none exists"),
        row(":r", "reload the diff"),
        row(":v", "voice mode (whisper.cpp)"),
        row(":pet", "pet Snappy 🦞"),
        row(":q", "quit"),
        Line::from(""),
    ];
    let _ = muted_style;
    let _ = accent_style;
    let width = area.width.min(88);
    let desired_height = (lines.len() as u16) + 1 + 2;
    let height = desired_height.min(area.height.saturating_sub(2)).max(10);
    let rect = centered_rect(area, width, height);
    f.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(PANEL))
        .title(Span::styled(" revu help ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let visible = sections[0].height as usize;
    let max_scroll = lines.len().saturating_sub(visible);
    let start = app.help_scroll.min(max_scroll);
    let end = (start + visible).min(lines.len());
    let visible_lines: Vec<Line> = lines[start..end].to_vec();
    f.render_widget(
        Paragraph::new(visible_lines)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(TEXT).bg(PANEL)),
        sections[0],
    );

    let muted_style = Style::default().fg(MUTED);
    let footer = Line::from(vec![
        Span::styled("  Esc / Enter", key_style),
        Span::styled(" close", muted_style),
        Span::raw("     "),
        Span::styled("?", key_style),
        Span::styled(" toggle", muted_style),
        Span::raw("     "),
        Span::styled("↑ ↓", key_style),
        Span::styled(" scroll", muted_style),
    ]);
    f.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(TEXT).bg(PANEL)),
        sections[1],
    );
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}
