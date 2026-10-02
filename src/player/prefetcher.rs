//! Keeps one downloaded track ready for each inactive genre.

use std::{collections::HashSet, fs, path::Path, sync::Arc};

use super::Player;
use crate::tracks::list::List;

/// Maximum number of inactive genres to warm in one pass.
const MAX_PREFETCH: usize = 8;

/// Built-in genres bundled with lazylofi.
const BUILTIN_GENRES: &[&str] = &["lofi", "synthwave", "jazz-lofi", "ambient"];

/// Warms the in-memory track pool for inactive genres.
pub struct Prefetcher;

impl Prefetcher {
    /// Download one track per inactive genre, unless it is already cached.
    pub async fn warm_all(player: Arc<Player>) {
        if player.has_custom_tracks {
            return;
        }

        let current = player.list.read().await.name.clone();
        let genres = collect_genres(&player.data_dir);

        if genres.len().saturating_sub(BUILTIN_GENRES.len()) > MAX_PREFETCH {
            if player.debug {
                eprintln!(
                    "warning: more than {MAX_PREFETCH} custom genres found; limiting prefetch"
                );
            }
        }

        for genre in genres
            .into_iter()
            .filter(|genre| genre != &current)
            .take(MAX_PREFETCH)
        {
            if player.prefetched.read().await.contains_key(&genre) {
                continue;
            }

            match List::load(&None, &Some(genre.clone()), &player.data_dir).await {
                Ok(list) => match list.random(&player.client).await {
                    Ok(track) => {
                        player.prefetched.write().await.insert(genre, track);
                    }
                    Err(error) => {
                        if player.debug {
                            eprintln!("prefetch '{genre}' download failed: {error}");
                        }
                    }
                },
                Err(error) => {
                    if player.debug {
                        eprintln!("prefetch '{genre}' load failed: {error}");
                    }
                }
            }
        }
    }
}

/// Return built-in and custom genres, excluding hidden files and micropop.txt.
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
