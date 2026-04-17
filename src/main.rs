mod app;
mod comments;
mod diff;
mod git;
mod publish;
mod snappy;
mod ui;
mod voice;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind},
    execute,
    style::{Color as TermColor, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};

use app::{App, Mode, Overlay};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).map(|s| s.as_str()).unwrap_or(".");
    let base_ref = args.get(2).cloned();

    let path_buf = std::path::PathBuf::from(path);
    let repo_root = match git::repo_root(Some(&path_buf)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("revu: error finding git repository at '{}' ({})", path, e);
            std::process::exit(1);
        }
    };

    let mut app = App::new(repo_root, base_ref)?;
    app.reload_diff();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        SetBackgroundColor(TermColor::Black),
        SetForegroundColor(TermColor::White),
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        ResetColor,
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("revu error: {e:?}");
    }
    Ok(())
}

fn run<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let mut dirty = true;
    loop {
        if dirty {
            terminal.draw(|f| ui::draw(f, app))?;
            dirty = false;
        }

        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    if handle_key(app, key.code, key.modifiers)? {
                        break;
                    }
                    dirty = true;
                }
                Event::Mouse(m) => match m.kind {
                    MouseEventKind::Down(button) => {
                        match button {
                            MouseButton::Left => app.on_drag_start(m.column, m.row),
                            MouseButton::Right => app.cancel_mouse_selection(),
                            _ => {}
                        }
                        if matches!(button, MouseButton::Left) {
                            app.on_click(m.column, m.row);
                        }
                        dirty = true;
                    }
                    MouseEventKind::Drag(button) => {
                        match button {
                            MouseButton::Left => {
                                app.on_drag_update(m.column, m.row);
                                dirty = true;
                            }
                            _ => {}
                        }
                    }
                    MouseEventKind::Up(button) => {
                        match button {
                            MouseButton::Left => app.on_drag_end(),
                            _ => {}
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        app.on_scroll(m.column, m.row, 1, ui::file_tree_row_count(app));
                        dirty = true;
                    }
                    MouseEventKind::ScrollUp => {
                        app.on_scroll(m.column, m.row, -1, ui::file_tree_row_count(app));
                        dirty = true;
                    }
                    _ => {}
                },
                _ => {}
            }
        } else if app.tick() {
            dirty = true;
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode, mods: KeyModifiers) -> Result<bool> {
    if app.overlay != Overlay::None && matches!(app.mode, Mode::Normal) {
        match (code, mods) {
            (KeyCode::Esc, _) | (KeyCode::Char('q'), _) => {
                app.close_overlay();
            }
            (KeyCode::Enter, _) => app.activate_overlay_selection(),
            (KeyCode::PageUp, _) => app.overlay_move(-8),
            (KeyCode::PageDown, _) => app.overlay_move(8),
            (KeyCode::Up, _) | (KeyCode::Char('k'), _) => app.overlay_move(-1),
            (KeyCode::Down, _) | (KeyCode::Char('j'), _) => app.overlay_move(1),
            _ => {}
        }
        return Ok(false);
    }

    match app.mode.clone() {
        Mode::Command(buf) => {
            let mut buf = buf;
            match code {
                KeyCode::Esc => app.mode = Mode::Normal,
                KeyCode::Enter => {
                    let cmd = buf.clone();
                    app.mode = Mode::Normal;
                    if app.execute_command(&cmd)? {
                        return Ok(true);
                    }
                }
                KeyCode::Backspace => {
                    buf.pop();
                    app.mode = Mode::Command(buf);
                }
                KeyCode::Char(c) => {
                    buf.push(c);
                    app.mode = Mode::Command(buf);
                }
                _ => {}
            }
            return Ok(false);
        }
        Mode::Comment(draft) => {
            let mut draft = draft;
            match code {
                KeyCode::Esc => app.mode = Mode::Normal,
                KeyCode::Enter => {
                    if mods.contains(KeyModifiers::ALT) || mods.contains(KeyModifiers::SHIFT) {
                        draft.push('\n');
                        app.mode = Mode::Comment(draft);
                    } else {
                        app.save_current_comment(draft);
                        app.mode = Mode::Normal;
                    }
                }
                KeyCode::Backspace => {
                    draft.pop();
                    app.mode = Mode::Comment(draft);
                }
                KeyCode::Char(c) => {
                    draft.push(c);
                    app.mode = Mode::Comment(draft);
                }
                _ => {}
            }
            return Ok(false);
        }
        Mode::Normal => {}
    }

    match (code, mods) {
        (KeyCode::Char(':'), _) => app.mode = Mode::Command(String::new()),
        (KeyCode::Char('?'), _) => app.open_help(),
        (KeyCode::Char('c'), KeyModifiers::NONE) => app.begin_comment(),
        (KeyCode::Char('x'), KeyModifiers::NONE) => app.delete_current_comment(),
        (KeyCode::Char('V'), _) => app.toggle_review_range(),
        (KeyCode::Char('r'), KeyModifiers::NONE) => app.reload_diff(),
        (KeyCode::Char('f'), KeyModifiers::NONE) => app.toggle_files_overlay(),
        (KeyCode::Char('m'), KeyModifiers::NONE) => app.toggle_comments_overlay(),
        (KeyCode::Char('p'), KeyModifiers::CONTROL) => app.toggle_files_overlay(),
        (KeyCode::Up, m) if m.contains(KeyModifiers::SHIFT) => app.move_file(-1),
        (KeyCode::Down, m) if m.contains(KeyModifiers::SHIFT) => app.move_file(1),
        (KeyCode::Up, m) if m.contains(KeyModifiers::ALT) => app.move_file(-1),
        (KeyCode::Down, m) if m.contains(KeyModifiers::ALT) => app.move_file(1),
        (KeyCode::BackTab, _) => app.move_file(-1),
        (KeyCode::Tab, _) => app.move_file(1),
        (KeyCode::Char('H'), _) => app.move_file(-1),
        (KeyCode::Char('L'), _) => app.move_file(1),
        (KeyCode::Char('J'), _) => app.jump_latest_diff(),
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => app.move_line(-1),
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => app.move_line(1),
        (KeyCode::Char('n'), KeyModifiers::NONE) => app.next_hunk(),
        (KeyCode::Char('N'), _) => app.prev_hunk(),
        (KeyCode::Char(']'), _) => app.next_hunk(),
        (KeyCode::Char('['), _) => app.prev_hunk(),
        (KeyCode::Char('}'), _) => app.jump_next_comment(),
        (KeyCode::Char('{'), _) => app.jump_prev_comment(),
        (KeyCode::Char('g'), KeyModifiers::NONE) => app.goto_top(),
        (KeyCode::Char('G'), _) => app.goto_bottom(),
        (KeyCode::PageDown, _) => app.move_line(20),
        (KeyCode::PageUp, _) => app.move_line(-20),
        (KeyCode::Esc, _) => app.close_overlay(),
        _ => {}
    }
    Ok(false)
}
