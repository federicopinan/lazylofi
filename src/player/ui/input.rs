//! Async input handler for the player UI.
//!
//! Reads crossterm [`KeyEvent`]s and either updates the shared
//! [`App`] (e.g. the picker overlay or volume timer) or sends a
//! [`Messages`] to the audio server.
//!
//! This loop never returns on a normal quit — quitting goes through
//! `Messages::Quit` which is handled by the audio server. The loop
//! only returns (with an error) if the crossterm event stream breaks.

use crossterm::event::{self, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures::{FutureExt as _, StreamExt as _};
use tokio::sync::mpsc::Sender;

use crate::player::Messages;

use super::{App, SharedApp};

/// What a picker key means once handled.
#[expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Variants are grouped by effect — Stay/Exit stays, Pick stays separate — rather than alphabetically. The enum is local to this module."
)]
enum PickerAction {
    /// Picker handled the key; stay open.
    Stay,
    /// Picker should close without submitting a genre.
    Cancel,
    /// Picker should close and submit the given genre.
    Pick(String),
}

/// What a player key means once handled.
#[expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Variants are grouped by effect (audio server vs. local UI), not alphabetically. The enum is local to this module."
)]
enum PlayerAction {
    /// Send a message to the audio server.
    Send(Messages),
    /// No-op (key fell through).
    Ignore,
    /// Open the picker overlay (no audio message).
    OpenPicker,
}

/// Runs the input loop until the event stream breaks.
///
/// `app` is the shared application state; `sender` delivers messages
/// to the audio server.
pub async fn listen(sender: Sender<Messages>, app: SharedApp) -> eyre::Result<()> {
    let mut reader = EventStream::new();

    loop {
        let Some(Ok(event::Event::Key(event))) = reader.next().fuse().await else {
            continue;
        };

        if event.kind == KeyEventKind::Release {
            continue;
        }

        // Picker takes precedence: while it's open, only picker inputs
        // are honored. Player keys fall through (continue) so the
        // audio server isn't poked while the modal is up.
        {
            let mut app_guard = app.write().await;
            if app_guard.show_picker {
                match handle_picker_key(&mut app_guard, event.code) {
                    PickerAction::Stay => {}
                    PickerAction::Cancel => {
                        app_guard.show_picker = false;
                    }
                    PickerAction::Pick(genre) => {
                        app_guard.show_picker = false;
                        // Drop the app lock before awaiting the sender
                        // so the render loop isn't blocked on the
                        // audio channel.
                        drop(app_guard);
                        if let Err(error) = sender.send(Messages::ChangeGenre(genre)).await {
                            eprintln!("failed to send ChangeGenre: {error}");
                        }
                    }
                }
            } else {
                let action = player_key_to_action(&mut app_guard, event.code, event.modifiers);
                drop(app_guard);
                if let PlayerAction::Send(msg) = action {
                    sender.send(msg).await?;
                }
            }
        }
    }
}

/// Translates a key event into picker behaviour while the modal is open.
fn handle_picker_key(app: &mut App, code: KeyCode) -> PickerAction {
    match code {
        KeyCode::Up | KeyCode::Char('k') => {
            let i = app.picker_state.selected().unwrap_or(0);
            app.picker_state.select(Some(i.saturating_sub(1)));
            PickerAction::Stay
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let i = app.picker_state.selected().unwrap_or(0);
            if i + 1 < app.picker_genres.len() {
                app.picker_state.select(Some(i + 1));
            }
            PickerAction::Stay
        }
        KeyCode::Enter => {
            let i = app.picker_state.selected().unwrap_or(0);
            app.picker_genres
                .get(i)
                .cloned()
                .map_or(PickerAction::Cancel, PickerAction::Pick)
        }
        KeyCode::Char('q') | KeyCode::Esc => PickerAction::Cancel,
        _ => PickerAction::Stay,
    }
}

/// Translates a player key into an action (send message, open picker, or ignore).
fn player_key_to_action(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> PlayerAction {
    match code {
        KeyCode::Up => {
            app.bump_volume_timer();
            PlayerAction::Send(Messages::ChangeVolume(0.1))
        }
        KeyCode::Right => {
            app.bump_volume_timer();
            PlayerAction::Send(Messages::ChangeVolume(0.01))
        }
        KeyCode::Down => {
            app.bump_volume_timer();
            PlayerAction::Send(Messages::ChangeVolume(-0.1))
        }
        KeyCode::Left => {
            app.bump_volume_timer();
            PlayerAction::Send(Messages::ChangeVolume(-0.01))
        }
        KeyCode::Char(character) => char_to_player_action(app, character, modifiers),
        KeyCode::Media(media) => media_to_player_action(app, media),
        _ => PlayerAction::Ignore,
    }
}

/// Translates a character key into a player action.
fn char_to_player_action(app: &mut App, character: char, modifiers: KeyModifiers) -> PlayerAction {
    match character.to_ascii_lowercase() {
        // Ctrl+C
        'c' if modifiers == KeyModifiers::CONTROL => PlayerAction::Send(Messages::Quit),

        // Quit
        'q' => PlayerAction::Send(Messages::Quit),

        // Skip/Next
        's' | 'n' => PlayerAction::Send(Messages::Next),

        // Pause
        'p' => PlayerAction::Send(Messages::PlayPause),

        // Volume up & down — bump the timer so the volume bar shows
        // up instead of the progress bar.
        '+' | '=' => {
            app.bump_volume_timer();
            PlayerAction::Send(Messages::ChangeVolume(0.1))
        }
        '-' | '_' => {
            app.bump_volume_timer();
            PlayerAction::Send(Messages::ChangeVolume(-0.1))
        }

        // Open the live genre picker. With `--tracks` the picker is
        // disabled and the key is a no-op.
        'g' => {
            if !app.player.has_custom_tracks {
                app.show_picker = true;
                app.picker_state.select(Some(0));
            }
            PlayerAction::OpenPicker
        }

        _ => PlayerAction::Ignore,
    }
}

/// Translates a media key into a player action.
const fn media_to_player_action(app: &mut App, media: event::MediaKeyCode) -> PlayerAction {
    match media {
        event::MediaKeyCode::Pause | event::MediaKeyCode::Play | event::MediaKeyCode::PlayPause => {
            PlayerAction::Send(Messages::PlayPause)
        }
        event::MediaKeyCode::Stop => PlayerAction::Send(Messages::Pause),
        event::MediaKeyCode::TrackNext => PlayerAction::Send(Messages::Next),
        event::MediaKeyCode::LowerVolume => {
            app.bump_volume_timer();
            PlayerAction::Send(Messages::ChangeVolume(-0.1))
        }
        event::MediaKeyCode::RaiseVolume => {
            app.bump_volume_timer();
            PlayerAction::Send(Messages::ChangeVolume(0.1))
        }
        event::MediaKeyCode::MuteVolume => {
            app.bump_volume_timer();
            PlayerAction::Send(Messages::ChangeVolume(-1.0))
        }
        _ => PlayerAction::Ignore,
    }
}
