//! Interactive TUI picker for choosing a genre.
//!
//! This module exposes two flows:
//!
//! - The legacy standalone [`pick`] function, used at app startup
//!   (before the player exists) by [`crate::play`]. It owns its own
//!   ratatui session via `ratatui::init()/restore()`.
//! - The new modal [`render_picker_modal`] function, used as an
//!   overlay on top of the running player frame. The state lives on
//!   the shared [`App`](super::app::App) and is driven by the main
//!   input handler.
//!
//! Both flows share the same genre-collection logic ([`collect_genres`])
//! and the same [`List`] widget rendering style.

use std::{fs, path::Path, time::Duration};

use eyre::eyre;
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style, Stylize as _},
    text::Span,
    widgets::{Block, Clear, List, ListItem, ListState},
    DefaultTerminal, Frame,
};

use super::app::App;
use super::components::{AMBER, BACKGROUND, GREEN};

/// Built-in genres bundled with lazylofi.
///
/// Single source of truth for the built-in genre name list. The
/// embedded list contents (the actual track URLs) live in
/// [`crate::tracks::list`] alongside their only consumer.
pub const BUILTIN_GENRES: &[&str] = &["lofi", "synthwave", "jazz-lofi", "ambient"];

/// File in `data_dir` that should be excluded from genre lists.
const EXCLUDED_FILE: &str = "micropop.txt";

/// Scans `data_dir` for genre `.txt` files, excluding hidden files and [`EXCLUDED_FILE`].
fn scan_data_dir(data_dir: &Path) -> eyre::Result<Vec<String>> {
    if !data_dir.exists() {
        return Ok(Vec::new());
    }

    let mut genres = Vec::new();
    for entry in fs::read_dir(data_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with('.') {
            continue;
        }
        if name_str == EXCLUDED_FILE {
            continue;
        }
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("txt") {
            continue;
        }

        if let Some(stem) = entry
            .path()
            .file_stem()
            .and_then(|stem_str| stem_str.to_str())
        {
            genres.push(stem.to_owned());
        }
    }

    Ok(genres)
}

/// Returns the available genres: built-ins plus user-added files from `data_dir`.
///
/// Public so that the [`App`](super::app::App) constructor can load the
/// list once at startup and reuse it for the live picker overlay.
pub fn collect_genres(data_dir: &Path) -> eyre::Result<Vec<String>> {
    let user_genres = scan_data_dir(data_dir)?;
    let mut genres: Vec<String> = BUILTIN_GENRES
        .iter()
        .map(|genre| (*genre).to_owned())
        .collect();
    for genre in user_genres {
        if !genres.contains(&genre) {
            genres.push(genre);
        }
    }
    Ok(genres)
}

/// Returns a rectangle of `percent_x` width and `height` rows, centered within `area`.
fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}

/// Draws the picker onto `frame`.
///
/// Used by both the standalone startup flow and the live overlay
/// (which delegates here so the styling stays consistent).
fn render(frame: &mut Frame, genres: &[String], state: &mut ListState) {
    let items: Vec<ListItem<'_>> = genres
        .iter()
        .map(|genre| ListItem::new(genre.as_str().fg(GREEN)))
        .collect();

    let area = centered_rect(60, genres.len() as u16 + 2, frame.area());

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(" Select a genre ".bold().fg(AMBER))
                .style(Style::default().fg(AMBER).bg(BACKGROUND)),
        )
        .style(Style::default().bg(BACKGROUND))
        .highlight_style(
            Style::default()
                .fg(BACKGROUND)
                .bg(AMBER)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(Span::styled("▶ ", Style::default().fg(AMBER)));

    frame.render_stateful_widget(list, area, state);
}

/// Renders the picker as a centered modal overlay on top of the
/// running player frame.
///
/// Reads the genre list and the selection state from [`App`], which
/// means this function only paints — the input handler is responsible
/// for updating `app.show_picker` and `app.picker_state` in response
/// to key events.
pub fn render_picker_modal(frame: &mut Frame, app: &mut App) {
    if app.picker_genres.is_empty() {
        // Nothing to show. Bail out so we don't render an empty box.
        return;
    }

    let items: Vec<ListItem<'_>> = app
        .picker_genres
        .iter()
        .map(|genre| ListItem::new(genre.as_str().fg(GREEN)))
        .collect::<Vec<_>>();

    let area = centered_rect(60, app.picker_genres.len() as u16 + 2, frame.area());

    // Wipe the area first so the modal has a clean background and
    // visually takes focus over the player.
    frame.render_widget(Clear, area);

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(" Select a genre ".bold().fg(AMBER))
                .style(Style::default().fg(AMBER).bg(BACKGROUND)),
        )
        .style(Style::default().bg(BACKGROUND))
        .highlight_style(
            Style::default()
                .fg(BACKGROUND)
                .bg(AMBER)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(Span::styled("▶ ", Style::default().fg(AMBER)));

    frame.render_stateful_widget(list, area, &mut app.picker_state);
}

/// Runs the picker event loop on `terminal` until the user picks or quits.
///
/// Used by the standalone [`pick`] function (startup picker).
/// Not used by the live overlay, which is driven by the main input
/// handler instead.
fn run_picker(terminal: &mut DefaultTerminal, genres: &[String]) -> eyre::Result<Option<String>> {
    let mut state = ListState::default().with_selected(Some(0));

    let result = loop {
        terminal.draw(|frame| render(frame, genres, &mut state))?;

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        let event = event::read()?;
        let Event::Key(key) = event else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                let i = state.selected().unwrap_or(0);
                state.select(Some(i.saturating_sub(1)));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let i = state.selected().unwrap_or(0);
                if i + 1 < genres.len() {
                    state.select(Some(i + 1));
                }
            }
            KeyCode::Enter => {
                let i = state.selected().unwrap_or(0);
                break Some(genres[i].clone());
            }
            KeyCode::Char('q') | KeyCode::Esc => break None,
            _ => {}
        }
    };

    Ok(result)
}

/// Shows a standalone TUI picker for the available genres and returns the chosen one.
///
/// This is the legacy startup picker used by [`crate::play`] before
/// the main player exists. It owns its own `ratatui::init()/restore()`
/// session — the live overlay does not go through this path.
///
/// Returns `None` if the user quits with `q` / `Esc`.
/// `data_dir` is the data directory to scan for additional `<genre>.txt` files.
///
/// # Errors
///
/// Returns an error if reading `data_dir` fails, no genre lists are found,
/// or terminal setup/rendering fails.
pub fn pick(data_dir: &Path) -> eyre::Result<Option<String>> {
    let genres = collect_genres(data_dir)?;

    if genres.is_empty() {
        return Err(eyre!("no genre lists found in {}", data_dir.display()));
    }

    let mut terminal = ratatui::init();
    let result = run_picker(&mut terminal, &genres);
    ratatui::restore();
    result
}
