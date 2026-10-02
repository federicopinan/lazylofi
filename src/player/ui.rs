//! The module which manages all user interface, including inputs.
//!
//! The UI is a single ratatui session owned by [`start`]. A render
//! task draws the player frame (and the picker overlay, when active)
//! at 12 FPS; the input handler runs in the foreground and updates
//! the shared [`App`](app::App) state.

mod app;
mod components;
mod input;
pub mod picker;

use std::{sync::Arc, time::Duration};

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    widgets::Block,
    Frame,
};
use tokio::{
    sync::{mpsc::Sender, RwLock},
    task,
    time::sleep,
};

use super::{LoadPhase, Messages, Player};
use crate::Args;
use app::{App, SharedApp};

/// Self explanitory.
const FPS: usize = 12;

/// How long to wait in between frames.
/// This is fairly arbitrary, but an ideal value should be enough to feel
/// snappy but not require too many resources.
const FRAME_DELTA: f32 = 1.0 / FPS as f32;

/// Split the radio face into frequency, dial, status, genre, track, progress, controls and the wave band.
fn chunks_for(app: &App, inner: Rect) -> Vec<Rect> {
    let constraints = if app.minimalist {
        vec![
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(2),
        ]
    } else {
        vec![
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(2),
        ]
    };

    Layout::vertical(constraints).split(inner).to_vec()
}

/// Renders one frame onto `frame`, given a write-locked `app`.
///
/// The picker overlay is drawn last so it sits on top of the player
/// frame. The render loop holds a write lock so [`render_picker_modal`]
/// can mutate `app.picker_state` if ratatui chooses to.
fn render_frame(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    let outer = Block::bordered().title(" lazylofi · FM STEREO ").style(
        Style::default()
            .fg(components::GREEN)
            .bg(components::BACKGROUND),
    );
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let chunks = chunks_for(app, inner);

    // Load `current` once so it isn't loaded multiple times by different
    // widgets. `ArcSwapOption::load` returns `Option<Arc<Info>>`; we
    // turn it into `Option<&Arc<Info>>` for the widgets.
    let current_arc = app.player.current.load();
    let current = current_arc.as_ref();

    frame.render_widget(components::frequency_display(app.current_freq), chunks[0]);
    let dial_freq = if app.load_phase == LoadPhase::Tuning {
        88.0 + ((u16::from(app.animation_tick) * 73 + 31) % 201) as f32 / 10.0
    } else {
        app.current_freq
    };
    frame.render_widget(
        components::dial(
            dial_freq,
            app.animation_tick,
            chunks[1].width as usize,
            app.load_phase == LoadPhase::Scanning,
        ),
        chunks[1],
    );
    frame.render_widget(
        components::load_status(app.load_phase, app.animation_tick, chunks[2].width as usize),
        chunks[2],
    );

    let progress_index = if app.minimalist {
        3
    } else {
        frame.render_widget(
            components::genre(&app.player, chunks[3].width as usize),
            chunks[3],
        );
        frame.render_widget(
            components::track(current, chunks[4].width as usize),
            chunks[4],
        );
        5
    };
    let progress = chunks[progress_index];
    if app.volume_timer > 0 {
        frame.render_widget(
            components::audio_bar(app.player.sink.volume(), progress.width as usize),
            progress,
        );
    } else {
        frame.render_widget(
            components::progress_bar(&app.player, current, progress.width as usize),
            progress,
        );
    }
    if !app.minimalist {
        frame.render_widget(
            components::controls(&app.player, chunks[6].width as usize),
            chunks[6],
        );
    }

    // The animated ocean band sits at the bottom in both modes so
    // even with the controls hidden the radio still has its horizon.
    let wave_index = if app.minimalist { 4 } else { 7 };
    let wave_area = chunks[wave_index];
    frame.render_widget(
        components::wave(app.animation_tick, wave_area.width as usize),
        wave_area,
    );

    if app.show_picker {
        picker::render_picker_modal(frame, app);
    }
}

/// Runs the render loop until the task is aborted.
///
/// Owns the [`ratatui::DefaultTerminal`] for the lifetime of the loop.
/// Calls `ratatui::init()` once and never `restore()`s — the caller
/// (see [`start`]) does that when the input handler returns.
async fn render_loop(app: SharedApp) -> eyre::Result<()> {
    let mut terminal = ratatui::init();

    loop {
        let mut app_guard = app.write().await;
        app_guard.tick_volume_timer();
        let phase = app_guard
            .player
            .load_phase
            .try_read()
            .map(|phase| *phase)
            .unwrap_or(app_guard.server_phase);
        app_guard.tick_load_phase(phase);
        terminal.draw(|frame| render_frame(frame, &mut app_guard))?;
        drop(app_guard);
        sleep(Duration::from_secs_f32(FRAME_DELTA)).await;
    }
}

/// Initializes the UI and runs both the render task and the input loop.
///
/// The render task owns its terminal session via `ratatui::init()`; we
/// call `ratatui::restore()` once on the way out, even if the input
/// handler errored.
pub async fn start(player: Arc<Player>, sender: Sender<Messages>, args: Args) -> eyre::Result<()> {
    // Load the picker genres up-front so the live overlay is responsive
    // when the user presses `g`. Failures are non-fatal: an empty list
    // just makes the overlay do nothing.
    let picker_genres = picker::collect_genres(&player.data_dir).unwrap_or_default();

    let app: SharedApp = Arc::new(RwLock::new(App::new(
        Arc::clone(&player),
        picker_genres,
        args.minimalist,
    )));

    // Spawn the render task. It owns its own terminal.
    let render_app = Arc::clone(&app);
    let render_handle = task::spawn(async move { render_loop(render_app).await });

    // The input handler runs in the foreground. `listen` only returns
    // on error — the normal "quit" path goes through `Messages::Quit`
    // which is handled by the audio server. `play::play` aborts this
    // task when the audio server returns.
    let result = input::listen(sender, Arc::clone(&app)).await;

    // Tear down the render task and restore terminal state. We always
    // restore, even when the input handler errored.
    render_handle.abort();
    ratatui::restore();

    result
}
