//! Keeps one downloaded track ready per genre.
//!
//! The prefetcher is standalone — it does not depend on a [`Player`] being
//! constructed — and is shared with [`Player`](super::Player) via the
//! `Arc<RwLock<HashMap>>` field so the first track for the chosen genre is
//! already downloaded (or close to it) when the picker closes.
//!
//! Two entry points:
//! - [`Prefetcher::warm_one`] is called per genre during the picker, so
//!   playback of the picked genre can start on a prefetched track.
//! - [`Prefetcher::warm_all`] is the original "everything you aren't
//!   playing" sweep, kept as a re-warm for genre switches and the
//!   post-pick refresh.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use reqwest::Client;
use tokio::sync::RwLock;

use crate::player::ui::picker::BUILTIN_GENRES;
use crate::tracks::{list::List, Track};

/// Maximum number of inactive genres to warm in one pass.
const MAX_PREFETCH: usize = 8;

/// Timeout used for the shared HTTP client. Shared between the picker-time
/// prefetch in [`play::play`](crate::play::play) and the in-flight downloads
/// inside [`Player`](super::Player).
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(5);

/// Warms the in-memory track pool: one downloaded [`Track`] per genre,
/// shared with the [`Player`](super::Player) so the first track of the
/// chosen genre is ready as soon as the user picks.
pub struct Prefetcher {
    /// HTTP client used for prefetch downloads. Shared with the player
    /// when it adopts the prefetcher, so both reuse the same connection pool.
    pub client: Client,

    /// Path to the data directory. Used both to reload genre lists on
    /// change and to resolve user-added `.txt` files.
    pub data_dir: PathBuf,

    /// Whether verbose diagnostic output (prefetch failures) is allowed
    /// to surface in the terminal.
    pub debug: bool,

    /// True when the user passed `--tracks`; the prefetcher is a no-op
    /// in that mode because the user committed to a custom list.
    pub has_custom_tracks: bool,

    /// The downloaded track per genre. Shared with [`Player`](super::Player)
    /// so the player's `next` path can consume a prefetched track for
    /// the current genre before falling back to a fresh download.
    pub prefetched: Arc<RwLock<HashMap<String, Track>>>,
}

impl Prefetcher {
    /// Builds a fresh prefetcher. Callers typically wrap this in `Arc`
    /// so the same instance can be moved into a background task and
    /// later adopted by the player.
    pub fn new(
        client: Client,
        data_dir: PathBuf,
        has_custom_tracks: bool,
        debug: bool,
    ) -> Self {
        Self {
            client,
            data_dir,
            has_custom_tracks,
            debug,
            prefetched: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Warms every inactive genre (the original behavior). Skip `current`
    /// so we don't race the player for the track it's about to play.
    /// `MAX_PREFETCH` caps the per-call work so a custom folder with
    /// thousands of `.txt` files doesn't tie up the runtime.
    pub async fn warm_all(self: Arc<Self>, current: Option<&str>) {
        if self.has_custom_tracks {
            return;
        }

        let genres = collect_genres(&self.data_dir);
        let capped = genres.len().saturating_sub(BUILTIN_GENRES.len()) > MAX_PREFETCH;
        if capped && self.debug {
            eprintln!("prefetch: more than {MAX_PREFETCH} custom genres found; limiting");
        }

        let mut warmed = 0;
        for genre in genres
            .into_iter()
            .filter(|genre| Some(genre.as_str()) != current)
        {
            if self.prefetched.read().await.contains_key(&genre) {
                continue;
            }
            if warmed >= MAX_PREFETCH {
                break;
            }
            Arc::clone(&self).warm_one(&genre).await;
            warmed += 1;
        }
    }

    /// Downloads one track for `genre` into the shared map, unless the
    /// genre is already warmed or the prefetcher is disabled.
    ///
    /// Errors are logged when `debug` is set; the function never
    /// returns a `Result` so the spawn-and-forget call sites stay
    /// simple.
    pub async fn warm_one(self: Arc<Self>, genre: &str) {
        if self.has_custom_tracks || genre.is_empty() {
            return;
        }
        if self.prefetched.read().await.contains_key(genre) {
            return;
        }

        match List::load(&None, &Some(genre.to_owned()), &self.data_dir).await {
            Ok(list) => match list.random(&self.client).await {
                Ok(track) => {
                    self.prefetched.write().await.insert(genre.to_owned(), track);
                }
                Err(error) => {
                    if self.debug {
                        eprintln!("prefetch '{genre}' download failed: {error}");
                    }
                }
            },
            Err(error) => {
                if self.debug {
                    eprintln!("prefetch '{genre}' load failed: {error}");
                }
            }
        }
    }
}

/// Builds the HTTP client used by both the prefetcher and the player, so
/// they share a connection pool and the same timeout/UA.
pub fn build_client() -> eyre::Result<Client> {
    Ok(Client::builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .timeout(HTTP_TIMEOUT)
        .build()?)
}

/// Returns built-in and custom genres, excluding hidden files and micropop.txt.
/// Tolerant of a missing data directory — returns just the built-ins in that case.
pub fn collect_genres(data_dir: &Path) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut genres = Vec::new();

    for builtin in BUILTIN_GENRES {
        if seen.insert((*builtin).to_owned()) {
            genres.push((*builtin).to_owned());
        }
    }

    if let Ok(entries) = fs::read_dir(data_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            // Genre files match the picker's exact, case-sensitive .txt extension.
            #[expect(
                clippy::case_sensitive_file_extension_comparisons,
                reason = "The genre picker only accepts lowercase .txt files"
            )]
            let is_genre_file = name.ends_with(".txt");
            if !is_genre_file || name.starts_with('.') || name == "micropop.txt" {
                continue;
            }

            let stem = name.trim_end_matches(".txt").to_owned();
            if seen.insert(stem.clone()) {
                genres.push(stem);
            }
        }
    }

    genres
}
