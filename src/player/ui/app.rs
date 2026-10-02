//! Application-level state for the TUI.
//!
//! This holds everything the render loop and input handler need to
//! share: a reference to the audio [`Player`], the picker overlay
//! state, and the volume-bar timer (formerly a global atomic).

use std::sync::Arc;

use ratatui::widgets::ListState;
use tokio::sync::RwLock;

use crate::player::{LoadPhase, Player};

/// Number of frames the volume bar stays visible after a volume change.
pub const AUDIO_BAR_DURATION_FRAMES: usize = 10;

/// Read-only handle used by the render loop.
///
/// The render loop holds `RwLockReadGuard` and never needs to mutate;
/// the input handler upgrades to `RwLockWriteGuard` to apply state
/// changes.
pub type SharedApp = Arc<RwLock<App>>;

/// All UI state shared between the render loop and the input handler.
///
/// Wrapped in `Arc<RwLock<App>>` so both tasks can read/mutate it.
#[expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Struct fields are grouped by UI concern (picker state, volume state, config), not alphabetically. Matches the convention used elsewhere in `player.rs`."
)]
pub struct App {
    /// Mirrors `args.minimalist`. When true, the bottom controls bar
    /// is omitted from the render frame.
    pub minimalist: bool,

    /// The audio server. The UI reads `current` and `sink.volume()`;
    /// the input handler checks `has_custom_tracks`.
    pub player: Arc<Player>,

    /// Genres currently available to the picker. Loaded once at startup;
    /// survives until the program exits.
    pub picker_genres: Vec<String>,

    /// The picker's selection state. Mutated by the input handler when
    /// the user presses `up`/`down`/`k`/`j`.
    pub picker_state: ListState,

    /// When true, the picker modal overlay is rendered on top of the
    /// player frame. The input handler routes keys to the picker while
    /// this is set.
    pub show_picker: bool,

    /// How many more frames the volume bar should stay on screen.
    /// Decremented each frame by [`App::tick_volume_timer`].
    /// When zero, the progress bar is shown instead.
    pub volume_timer: usize,

    /// Current dial state, copied from the audio server before drawing.
    pub load_phase: LoadPhase,
    /// 12 FPS animation counter for the spinner and scanning needle.
    pub animation_tick: u8,
    /// Frequency of the last successfully tuned track.
    pub current_freq: f32,
    /// Frames spent in the current visible loading phase.
    pub phase_frames: u8,
    /// Last loading state received from the audio server.
    pub server_phase: LoadPhase,
}

#[expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Methods are ordered by call frequency (new, then hot path), not alphabetically. Matches the convention used elsewhere in `player.rs`."
)]
impl App {
    /// Builds an `App` from a [`Player`] and the picked list.
    ///
    /// `minimalist` mirrors `args.minimalist` and controls whether the
    /// controls line is rendered.
    /// `picker_genres` is loaded once at startup from the data dir.
    pub fn new(player: Arc<Player>, picker_genres: Vec<String>, minimalist: bool) -> Self {
        Self {
            player,
            show_picker: false,
            picker_genres,
            picker_state: ListState::default().with_selected(Some(0)),
            volume_timer: 0,
            load_phase: LoadPhase::Scanning,
            animation_tick: 0,
            current_freq: 88.0,
            phase_frames: 0,
            server_phase: LoadPhase::Scanning,
            minimalist,
        }
    }

    /// Decrements the volume timer; called every frame from the render
    /// loop. Saturates at zero.
    pub const fn tick_volume_timer(&mut self) {
        if self.volume_timer > 0 {
            self.volume_timer = self.volume_timer.saturating_sub(1);
        }
    }

    /// Advance one animation frame, wrapping without overflow.
    pub fn tick_load_phase(&mut self, server_phase: LoadPhase) {
        self.animation_tick = self.animation_tick.wrapping_add(1);
        if server_phase == LoadPhase::Scanning && self.server_phase != LoadPhase::Scanning {
            self.load_phase = LoadPhase::Scanning;
            self.phase_frames = 0;
        }
        self.server_phase = server_phase;
        self.phase_frames = self.phase_frames.saturating_add(1);
        if self.phase_frames >= 6 {
            let next = match (self.load_phase, server_phase) {
                (LoadPhase::Scanning, LoadPhase::Tuning | LoadPhase::Locked { .. }) => {
                    Some(LoadPhase::Tuning)
                }
                (LoadPhase::Tuning, LoadPhase::Locked { freq }) => Some(LoadPhase::Locked { freq }),
                _ => None,
            };
            if let Some(phase) = next {
                self.load_phase = phase;
                self.phase_frames = 0;
            }
        }
        if let LoadPhase::Locked { freq } = self.load_phase {
            self.current_freq = freq;
        }
    }

    /// Bumps the volume timer when the user changes the volume, so the
    /// next [`AUDIO_BAR_DURATION_FRAMES`] frames render the volume bar.
    pub const fn bump_volume_timer(&mut self) {
        self.volume_timer = AUDIO_BAR_DURATION_FRAMES;
    }
}
