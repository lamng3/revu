use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
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
const ADD_FG: Color = Color::LightGreen;
const DEL_FG: Color = Color::LightRed;
const CODE_BG: Color = Color::Indexed(231);
const CODE_TEXT: Color = Color::Black;
const CODE_MUTED: Color = Color::DarkGray;
const CODE_ADD_FG: Color = Color::Black;
const CODE_ADD_BG: Color = Color::Indexed(194);
const CODE_DEL_FG: Color = Color::Black;
const CODE_DEL_BG: Color = Color::Indexed(224);
const CODE_COMMENT: Color = Color::Blue;
const HUNK_FG: Color = Color::Blue;
const HUNK_BG: Color = Color::Indexed(195);
const SEL_BG: Color = Color::DarkGray;
const CODE_SEL_BG: Color = Color::Indexed(252);
const RANGE_BG: Color = Color::Indexed(195);
const RANGE_EDGE: Color = Color::Blue;
const RANGE_FG: Color = Color::Blue;
const COMMENT_DRAFT: Color = Color::Yellow;
const COMMENT_PUBLISHED: Color = Color::Green;
const COMMENT_ORPHAN: Color = Color::Yellow;
const DIFF_GUTTER_WIDTH: u16 = 18;

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BG)), size);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(8), Constraint::Length(2)])
        .split(size);

    let max_file_width = layout[1].width.saturating_sub(40).max(20);
    let file_panel_width = app.file_panel_width.clamp(20, max_file_width);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(file_panel_width),
            Constraint::Min(40),
        ])
        .split(layout[1]);

    app.click.file_rows.clear();
    app.click.folder_rows.clear();
    app.click.comment_rows.clear();
    app.click.diff_rows.clear();
    app.click.file_panel_bounds = None;
    app.click.diff_panel_bounds = None;
    app.click.horizontal_scrollbar_bounds = None;
    app.click.body_bounds = Some((
        layout[1].x,
        layout[1].y,
        layout[1].width,
        layout[1].height,
    ));
    app.click.file_resize_col = Some(body[0].x + body[0].width.saturating_sub(1));

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
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG))
        .title(Span::styled(" revu ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    f.render_widget(block, area);

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
        Span::styled("base ", Style::default().fg(MUTED)),
        Span::styled(app.base_label(), Style::default().fg(TEXT)),
        Span::raw("  "),
        Span::styled(format!("drafts {}", app.unpublished_count()), Style::default().fg(COMMENT_DRAFT)),
        Span::raw("    "),
        Span::styled("🦞 ", Style::default().fg(Color::LightRed)),
        Span::styled(snappy_message, Style::default().fg(MUTED)),
    ]);

    f.render_widget(
        Paragraph::new(info).style(Style::default().bg(BG)),
        inner,
    );
}

fn draw_files_panel(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG))
        .title(Span::styled(
            " files ↔ ",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ));
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
        .borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(PANEL))
        .border_style(Style::default().fg(BORDER))
        .title(Span::styled(
            " review ",
            Style::default().fg(TEXT).bg(PANEL).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.click.diff_panel_bounds = Some((inner.x, inner.y, inner.width, inner.height));

    let Some(file) = app.current_file().cloned() else {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "no diff to show",
                Style::default().fg(MUTED),
            )))
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
    app.diff_code_width = inner.width.saturating_sub(DIFF_GUTTER_WIDTH).max(1);
    let max_line_width = file
        .lines
        .iter()
        .map(|line| line.text.chars().count())
        .max()
        .unwrap_or(0);
    let max_horizontal_scroll =
        max_line_width.saturating_sub(app.diff_code_width as usize);
    app.diff_horizontal_scroll = app.diff_horizontal_scroll.min(max_horizontal_scroll);

    let (adds, dels) = app.file_change_counts(app.file_idx);
    let mut header_spans = vec![
        Span::styled(
            file.path.clone(),
            Style::default()
                .fg(CODE_TEXT)
                .bg(CODE_BG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("+{adds}"),
            Style::default().fg(CODE_ADD_FG).bg(CODE_BG),
        ),
        Span::raw(" "),
        Span::styled(
            format!("-{dels}"),
            Style::default().fg(CODE_DEL_FG).bg(CODE_BG),
        ),
        Span::raw("  "),
        Span::styled(
            format!("line {}/{}", app.line_idx.saturating_add(1), file.lines.len().max(1)),
            Style::default().fg(CODE_MUTED).bg(CODE_BG),
        ),
        Span::raw("  "),
        Span::styled(
            if app.full_file_view {
                "full file"
            } else {
                "changes"
            },
            Style::default().fg(HUNK_FG).bg(CODE_BG),
        ),
    ];
    if app.diff_horizontal_scroll > 0 {
        header_spans.push(Span::raw("  "));
        header_spans.push(Span::styled(
            format!("↔ col {}", app.diff_horizontal_scroll + 1),
            Style::default().fg(HUNK_FG).bg(CODE_BG),
        ));
    }
    if app.multiline_active() {
        if let Some((start, end)) = app.multiline_range() {
            header_spans.push(Span::raw("  "));
            header_spans.push(Span::styled(
                format!("▌ range {}–{} ({} lines)", start, end, end - start + 1),
                Style::default()
                    .fg(RANGE_EDGE)
                    .bg(CODE_BG)
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }
    let header = Line::from(header_spans);
    f.render_widget(
        Paragraph::new(header).style(Style::default().bg(CODE_BG)),
        Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
    );
    draw_right_border_cell(f, inner.x + inner.width, inner.y, CODE_BG);

    if app.wrap_code && app.line_idx >= app.scroll {
        let rows_to_selection: usize = file.lines[app.scroll..=app.line_idx]
            .iter()
            .map(|line| wrapped_line_height(line, app.diff_code_width))
            .sum();
        if rows_to_selection > diff_h as usize {
            app.scroll = app.line_idx;
        }
    }
    let start = app.scroll;
    let mut idx = start;
    let mut row_offset = 0u16;
    let mut selected_visible = false;

    while idx < file.lines.len() && row_offset < diff_h {
        let draw_y = inner.y + 1 + row_offset;
        let line = &file.lines[idx];
        let consumed = draw_diff_row(
            f,
            Rect {
                x: inner.x,
                y: draw_y,
                width: inner.width,
                height: diff_h - row_offset,
            },
            app,
            line,
            idx,
            idx == app.line_idx,
        );
        for wrapped_row in 0..consumed {
            app.click.diff_rows.push((draw_y + wrapped_row, idx));
        }
        if idx == app.line_idx {
            selected_visible = true;
        }
        row_offset += consumed;
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

    if !app.wrap_code && max_line_width > app.diff_code_width as usize {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
            .thumb_symbol("━")
            .track_symbol(Some("─"))
            .thumb_style(Style::default().fg(Color::Gray))
            .track_style(Style::default().fg(BORDER))
            .begin_symbol(Some("‹"))
            .end_symbol(Some("›"));
        let mut scrollbar_state = ScrollbarState::new(max_line_width)
            .position(app.diff_horizontal_scroll)
            .viewport_content_length(app.diff_code_width as usize);
        let scrollbar_area = Rect {
            x: inner.x + DIFF_GUTTER_WIDTH,
            y: area.y + area.height.saturating_sub(1),
            width: app.diff_code_width,
            height: 1,
        };
        f.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        app.click.horizontal_scrollbar_bounds = Some((
            scrollbar_area.x.saturating_add(1),
            scrollbar_area.y,
            scrollbar_area.width.saturating_sub(2),
        ));
    }
}

pub fn file_tree_row_count(app: &App) -> usize {
    build_file_tree_rows(app).len()
}

fn wrapped_line_height(line: &DiffLine, code_width: u16) -> usize {
    let width = code_width.max(1) as usize;
    line.text.chars().count().max(1).div_ceil(width)
}

fn draw_diff_row(
    f: &mut Frame,
    area: Rect,
    app: &App,
    line: &DiffLine,
    idx: usize,
    selected: bool,
) -> u16 {
    let (fg, bg, marker) = style_for(line);
    let marker_style = comment_marker_style(app.comment_marker_for_line(line));
    let old_n = line.old_lineno.map(|n| format!("{n:>4}")).unwrap_or_else(|| "    ".to_string());
    let new_n = line.new_lineno.map(|n| format!("{n:>4}")).unwrap_or_else(|| "    ".to_string());
    let in_range = app.line_in_selected_range(idx);
    let row_bg = if in_range {
        RANGE_BG
    } else if let Some(change_bg) = bg {
        change_bg
    } else if selected {
        CODE_SEL_BG
    } else {
        CODE_BG
    };
    let range_edge = if in_range { '█' } else { ' ' };
    let edge_style = if in_range {
        Style::default().fg(RANGE_EDGE).bg(row_bg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(CODE_MUTED).bg(row_bg)
    };
    let number_style = if selected {
        Style::default().fg(CODE_TEXT).bg(row_bg).add_modifier(Modifier::BOLD)
    } else if in_range {
        Style::default().fg(RANGE_FG).bg(row_bg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(CODE_MUTED).bg(row_bg)
    };
    let text_style = if selected {
        Style::default().fg(fg).bg(row_bg).add_modifier(Modifier::BOLD)
    } else if in_range {
        Style::default().fg(fg).bg(row_bg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(fg).bg(row_bg)
    };

    let visible_text: String = line
        .text
        .chars()
        .skip(if app.wrap_code {
            0
        } else {
            app.diff_horizontal_scroll
        })
        .collect();
    let code_width = app.diff_code_width.max(1) as usize;
    let mut code_rows: Vec<String> = if app.wrap_code {
        visible_text
            .chars()
            .collect::<Vec<_>>()
            .chunks(code_width)
            .map(|chunk| chunk.iter().collect())
            .collect()
    } else {
        vec![visible_text]
    };
    if code_rows.is_empty() {
        code_rows.push(String::new());
    }

    let rows = code_rows.len().min(area.height as usize).max(1);
    let tree_glyph = app.comment_tree_glyph_for_line(line).unwrap_or(' ');
    for (wrapped_idx, code) in code_rows.into_iter().take(rows).enumerate() {
        let spans = if wrapped_idx == 0 {
            vec![
                Span::styled(format!("{range_edge}"), edge_style),
                Span::styled(format!(" {old_n} {new_n} "), number_style),
                Span::styled(format!(" {tree_glyph} "), marker_style.bg(row_bg)),
                Span::styled(format!(" {marker} "), text_style),
                Span::styled(code, text_style),
            ]
        } else {
            vec![
                Span::styled(" ".repeat(DIFF_GUTTER_WIDTH as usize), number_style),
                Span::styled(code, text_style),
            ]
        };
        f.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(row_bg)),
            Rect {
                x: area.x,
                y: area.y + wrapped_idx as u16,
                width: area.width,
                height: 1,
            },
        );
        draw_right_border_cell(
            f,
            area.x + area.width,
            area.y + wrapped_idx as u16,
            row_bg,
        );
    }
    rows as u16
}

fn draw_comment_row(
    f: &mut Frame,
    area: Rect,
    body: String,
    marker: Option<char>,
    selected_line: bool,
) {
    let bg = if selected_line { CODE_SEL_BG } else { CODE_BG };
    let label = match marker {
        Some('P') => "published",
        Some('!') => "orphaned",
        _ => "draft",
    };
    let style = match marker {
        Some('P') => Style::default().fg(Color::Green).bg(bg),
        Some('!') => Style::default().fg(Color::Red).bg(bg),
        _ => Style::default().fg(CODE_COMMENT).bg(bg),
    };
    let preview: String = body.chars().take(area.width.saturating_sub(24) as usize).collect();
    let line = Line::from(vec![
        Span::styled("             ", Style::default().bg(bg)),
        Span::styled("└─ ", style.add_modifier(Modifier::BOLD)),
        Span::styled("review ", style.add_modifier(Modifier::BOLD)),
        Span::styled(format!("[{label}] "), style),
        Span::styled(preview, Style::default().fg(CODE_TEXT).bg(bg)),
    ]);
    f.render_widget(Paragraph::new(line).style(Style::default().bg(bg)), area);
    draw_right_border_cell(f, area.x + area.width, area.y, bg);
}

fn draw_right_border_cell(f: &mut Frame, x: u16, y: u16, bg: Color) {
    f.render_widget(
        Paragraph::new(Span::styled("│", Style::default().fg(BORDER).bg(bg))),
        Rect {
            x,
            y,
            width: 1,
            height: 1,
        },
    );
}

fn draw_inline_comment_editor(f: &mut Frame, area: Rect, app: &App) {
    let draft = match &app.mode {
        Mode::Comment(draft) => draft.clone(),
        _ => return,
    };
    let preview = draft.replace('\n', " \\ ");
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(CODE_MUTED))
        .style(Style::default().bg(CODE_BG));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = vec![
        Line::from(vec![
            Span::styled("comment ", Style::default().fg(CODE_COMMENT).add_modifier(Modifier::BOLD)),
            Span::styled("enter save", Style::default().fg(CODE_MUTED)),
            Span::raw("  "),
            Span::styled("esc cancel", Style::default().fg(CODE_MUTED)),
            Span::raw("  "),
            Span::styled("shift+enter newline", Style::default().fg(CODE_MUTED)),
        ]),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(CODE_COMMENT).add_modifier(Modifier::BOLD)),
            Span::styled(preview, Style::default().fg(CODE_TEXT)),
            Span::styled("▎", Style::default().fg(CODE_COMMENT)),
        ]),
    ];
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).style(Style::default().bg(CODE_BG)),
        inner,
    );
}

fn comment_marker_style(marker: Option<char>) -> Style {
    match marker {
        Some('D') => Style::default().fg(CODE_COMMENT).add_modifier(Modifier::BOLD),
        Some('P') => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        Some('!') => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(CODE_MUTED),
    }
}

fn style_for(line: &DiffLine) -> (Color, Option<Color>, &'static str) {
    match line.kind {
        LineKind::Add => (CODE_ADD_FG, Some(CODE_ADD_BG), " "),
        LineKind::Del => (CODE_DEL_FG, Some(CODE_DEL_BG), " "),
        LineKind::Context => (CODE_TEXT, None, " "),
        LineKind::HunkHeader => (HUNK_FG, Some(HUNK_BG), "@"),
    }
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
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
            Span::raw("    "),
            Span::styled("↑↓", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(" move  ", Style::default().fg(MUTED)),
            Span::styled("←→", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(" pan  ", Style::default().fg(MUTED)),
            Span::styled("a", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(" view  ", Style::default().fg(MUTED)),
            Span::styled("Tab", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(" file  ", Style::default().fg(MUTED)),
            Span::styled("Enter", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(" comment  ", Style::default().fg(MUTED)),
            Span::styled("⇧R", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(" reload  ", Style::default().fg(MUTED)),
            Span::styled("?", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(" help  ", Style::default().fg(MUTED)),
            Span::styled("q", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(" quit", Style::default().fg(MUTED)),
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
        heading("  READ"),
        row("↑ ↓", "move through the diff"),
        row("← → / ⇧wheel", "pan code; disables wrapping"),
        row("drag bottom", "move the horizontal scrollbar"),
        row("w", "toggle code wrapping"),
        row("a", "toggle full file / changes"),
        row("Tab / ⇧Tab", "next / previous file"),
        row("drag ↔", "resize the files panel"),
        row("g / G", "top / bottom"),
        Line::from(""),
        heading("  REVIEW"),
        row("Enter / c", "add or edit a comment"),
        row("⇧↑ / ⇧↓", "extend a line selection"),
        row("Delete / x", "remove a draft comment"),
        row("f / m", "browse files / comments"),
        Line::from(""),
        heading("  APP"),
        row("⇧R", "reload changes"),
        row("? / q", "help / quit"),
        row(":p", "publish draft comments"),
        row(":pr [title]", "create a pull request"),
        row(":fetch", "fetch origin and reload"),
        row(":v", "dictate a comment"),
    ];
    let _ = muted_style;
    let _ = accent_style;
    let width = area.width.min(66);
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
