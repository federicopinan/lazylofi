//! Responsible for the basic initialization & shutdown of the audio server & frontend.

use std::path::PathBuf;
use std::sync::Arc;

use eyre::eyre;
use tokio::fs;
use tokio::{sync::mpsc, task};

use crate::paths;
use crate::player::ui::picker::BUILTIN_GENRES;
use crate::player::{ui, Messages};
use crate::player::{LoadPhase, Player};
use crate::player::prefetcher::{build_client, Prefetcher};
use crate::Args;

/// This is the representation of the persistent volume,
/// which is loaded at startup and saved on shutdown.
#[derive(Clone, Copy)]
pub struct PersistentVolume {
    /// The volume, as a percentage.
    inner: u16,
}

impl PersistentVolume {
    /// Retrieves the config directory.
    async fn config() -> eyre::Result<PathBuf> {
        paths::config_dir().await
    }

    /// Returns the volume as a percentage in the 0-100 range.
    pub const fn inner(self) -> u16 {
        self.inner
    }

    /// Loads the [`PersistentVolume`] from the lazylofi config directory.
    pub async fn load() -> eyre::Result<Self> {
        let config = Self::config().await?;
        let volume = config.join(PathBuf::from("volume.txt"));

        // Basically just read from the volume file if it exists, otherwise return 100.
        let volume = if volume.exists() {
            let contents = fs::read_to_string(volume).await?;
            let trimmed = contents.trim();
            let stripped = trimmed.strip_suffix("%").unwrap_or(trimmed);
            stripped
                .parse()
                .map_err(|_error| eyre!("volume.txt file is invalid"))?
        } else {
            fs::write(&volume, "100").await?;
            100u16
        };

        Ok(Self { inner: volume })
    }

    /// Saves `volume` to `volume.txt`.
    pub async fn save(volume: f32) -> eyre::Result<()> {
        let config = Self::config().await?;
        let path = config.join(PathBuf::from("volume.txt"));

        fs::write(path, ((volume * 100.0).abs().round() as u16).to_string()).await?;

        Ok(())
    }
}

/// Per-genre persistent volume.
///
/// Each genre keeps its own volume under
/// `~/.config/lazylofi/volume/<genre>.txt`. The file holds a single
/// integer in the 0-100 range. Missing files default to `100`.
/// The global [`PersistentVolume`] is kept as the fallback for the
/// `--tracks` case, where there is no genre to key on.
pub struct PersistentGenreVolume;

impl PersistentGenreVolume {
    /// Returns the per-genre volume directory, creating it if needed.
    async fn dir() -> eyre::Result<PathBuf> {
        paths::volume_dir().await
    }

    /// Validates a genre name so it can safely be used as a path
    /// component. Allows the four built-ins (`lofi`, `synthwave`,
    /// `jazz-lofi`, `ambient`) and any user-added genre whose filename
    /// is `[a-zA-Z0-9_-]+`.
    fn is_valid_name(name: &str) -> bool {
        !name.is_empty()
            && !name.contains("..")
            && !name.contains('/')
            && !name.contains('\\')
            && !name.contains('\0')
            && name
                .chars()
                .all(|chr| chr.is_ascii_alphanumeric() || chr == '-' || chr == '_')
    }

    /// Loads the volume for `genre`. Returns `100` if the file does not
    /// exist. Returns an error if the file exists but cannot be parsed
    /// as a `u16`.
    pub async fn load(genre: &str) -> eyre::Result<u16> {
        let path = Self::path(genre).await?;

        if !path.exists() {
            return Ok(100);
        }

        let contents = fs::read_to_string(&path).await?;
        let trimmed = contents.trim();
        let stripped = trimmed.strip_suffix("%").unwrap_or(trimmed);
        let volume: u16 = stripped
            .parse()
            .map_err(|_error| eyre!("volume/{genre}.txt file is invalid"))?;

        Ok(volume)
    }

    /// Returns the path for a given genre's volume file.
    async fn path(genre: &str) -> eyre::Result<PathBuf> {
        if !Self::is_valid_name(genre) {
            return Err(eyre!(
                "invalid genre name '{genre}': must be non-empty and contain only \
                 [a-zA-Z0-9_-] (no '..', '/', '\\', or NUL)"
            ));
        }
        Ok(Self::dir().await?.join(format!("{genre}.txt")))
    }

    /// Saves `volume` (0.0-1.0) to the genre's volume file.
    pub async fn save(genre: &str, volume: f32) -> eyre::Result<()> {
        let path = Self::path(genre).await?;

        let value = (volume * 100.0).abs().round() as u16;
        fs::write(path, value.to_string()).await?;

        Ok(())
    }
}

/// This is the representation of the persistent genre,
/// which is loaded at startup and saved after the genre is resolved.
///
/// Stored as a single line in `~/.config/lazylofi/genre.txt`.
pub struct PersistentGenre;

impl PersistentGenre {
    /// Retrieves the path to the genre file, creating the directory if missing.
    async fn config() -> eyre::Result<PathBuf> {
        Ok(paths::config_dir().await?.join(PathBuf::from("genre.txt")))
    }

    /// Loads the persisted genre from the lazylofi config directory.
    ///
    /// Returns `Ok(None)` when the file does not exist or is empty / whitespace-only.
    /// I/O or parse errors are surfaced so the caller can fall back to the picker.
    pub async fn load() -> eyre::Result<Option<String>> {
        let path = Self::config().await?;

        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&path).await?;
        let trimmed = contents.trim().to_owned();
        Ok(if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        })
    }

    /// Saves `genre` to `genre.txt`, replacing any previous value.
    pub async fn save(genre: &str) -> eyre::Result<()> {
        let path = Self::config().await?;
        fs::write(path, genre).await?;
        Ok(())
    }
}

/// Initializes the audio server, and then safely stops
/// it when the frontend quits.
pub async fn play(args: Args) -> eyre::Result<()> {
    // Build the prefetcher before the picker so background downloads
    // start the moment the user is staring at the genre list — the
    // picked genre's first track is on disk (or close to it) by the
    // time the picker closes.
    let data_dir = paths::data_dir().await?;
    let client = build_client()?;
    let has_custom_tracks = args.tracks.is_some();
    let prefetcher = Arc::new(Prefetcher::new(
        client,
        data_dir.clone(),
        has_custom_tracks,
        args.debug,
    ));

    // Kick off picker-time prefetch. With a persisted genre the only
    // target is that one; without one (or if the file is unreadable)
    // we fan out across the built-ins so whichever the user picks is
    // already on disk.
    if !has_custom_tracks {
        let warm_prefetcher = Arc::clone(&prefetcher);
        let target_genres: Vec<String> =
            match PersistentGenre::load().await {
                Ok(Some(genre)) => vec![genre],
                Ok(None) | Err(_) => BUILTIN_GENRES
                    .iter()
                    .map(|genre| (*genre).to_owned())
                    .collect(),
            };
        task::spawn(async move {
            for genre in target_genres {
                Arc::clone(&warm_prefetcher).warm_one(&genre).await;
            }
        });
    }

    // Resolve which genre to play, or whether to show the picker.
    let genre = match (args.genre.clone(), args.tracks.clone()) {
        (Some(genre), _) => Some(genre),
        (None, Some(_)) => None,
        (None, None) => {
            if let Ok(Some(genre)) = PersistentGenre::load().await {
                Some(genre)
            } else {
                // Either no persisted file, or it is empty / unreadable: fall
                // through to the picker. Persistence failures shouldn't block
                // the user from launching lazylofi.
                ui::picker::pick(&data_dir)?
            }
        }
    };

    // Persist the resolved genre so the next launch can skip the picker.
    // Skipped for `--tracks`: a custom list isn't a genre.
    if args.tracks.is_none() {
        if let Some(genre) = &genre {
            if let Err(error) = PersistentGenre::save(genre).await {
                eprintln!("warning: failed to persist genre '{genre}': {error}");
            }
        }
    }

    // The user pressed q/Esc in the picker — exit cleanly.
    if genre.is_none() && args.tracks.is_none() {
        return Ok(());
    }

    // Actually initializes the player. The adopted prefetcher means
    // `Player::next` consumes a prefetched track for the chosen genre
    // before falling back to a fresh download — no double-download of
    // the first song.
    let player = Arc::new(Player::new(&args, genre.clone(), Arc::clone(&prefetcher)).await?);

    let (tx, rx) = mpsc::channel(8);
    let ui = task::spawn(ui::start(Arc::clone(&player), tx.clone(), args));

    tx.send(Messages::LoadPhaseChanged(LoadPhase::Scanning))
        .await?;

    // Sends the player an "init" signal telling it to start playing a song straight away.
    tx.send(Messages::Init).await?;

    // Actually starts the player.
    Player::play(Arc::clone(&player), tx.clone(), rx).await?;

    // Save the volume.txt file for the next session.
    let final_volume = player.sink.volume();
    PersistentVolume::save(final_volume).await?;
    player.sink.stop();

    // Persist the per-genre volume too, so each genre keeps its own.
    // Skipped for `--tracks`: there is no genre to key on.
    if let Some(genre) = &genre {
        if let Err(error) = PersistentGenreVolume::save(genre, final_volume).await {
            eprintln!("warning: failed to persist per-genre volume: {error}");
        }
    }

    ui.abort();

    Ok(())
}
