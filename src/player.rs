//! Responsible for playing & queueing audio.
//! This also has the code for the underlying
//! audio server which adds new tracks.

use std::{
    collections::{HashMap, VecDeque},
    ffi::CString,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use arc_swap::ArcSwapOption;
use downloader::Downloader;
use libc::freopen;
use prefetcher::Prefetcher;
use reqwest::Client;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use tokio::{
    select,
    sync::{
        mpsc::{Receiver, Sender},
        RwLock,
    },
    task,
    time::sleep,
};

#[cfg(feature = "mpris")]
use mpris_server::{PlaybackStatus, PlayerInterface, Property};

use crate::{
    play::{PersistentGenreVolume, PersistentVolume},
    tracks::{self, list::List},
    Args,
};

pub mod downloader;
pub mod error;
pub mod prefetcher;
pub mod ui;

#[cfg(feature = "mpris")]
pub mod mpris;

/// Visual state of the radio dial while a track is being loaded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoadPhase {
    Scanning,
    Tuning,
    Locked { freq: f32 },
}

/// Maps a track name to a stable FM frequency in tenths of a MHz.
pub fn frequency_for(slug: &str) -> f32 {
    let hash = slug.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    (880 + hash % 201) as f32 / 10.0
}

#[cfg(test)]
mod frequency_tests {
    use super::frequency_for;

    #[test]
    fn frequency_is_stable_and_within_fm_band() {
        for name in ["midnight rain", "jazz lofi", "ambient", "", "雨"] {
            let frequency = frequency_for(name);
            assert_eq!(frequency, frequency_for(name));
            assert!((88.0..=108.0).contains(&frequency));
            assert_eq!(frequency * 10.0, (frequency * 10.0).round());
        }
    }
}

/// Handles communication between the frontend & audio player.
#[derive(PartialEq, Debug, Clone)]
#[allow(
    clippy::arbitrary_source_item_ordering,
    reason = "Variants are grouped by purpose (tracks / playback / volume / quit), not alphabetically. This ordering predates this change."
)]
pub enum Messages {
    /// Notifies the audio server that it should update the track.
    Next,

    /// Special in that this isn't sent in a "client to server" sort of way,
    /// but rather is sent by a child of the server when a song has not only
    /// been requested but also downloaded aswell.
    NewSong,

    /// This signal is only sent if a track timed out. In that case,
    /// lazylofi will try again and again to retrieve the track.
    TryAgain,

    /// Similar to Next, but specific to the first track.
    Init,

    /// Unpause the [Sink].
    Play,

    /// Pauses the [Sink].
    Pause,

    /// Pauses the [Sink]. This will also unpause it if it is paused.
    PlayPause,

    /// Change the volume of playback.
    ChangeVolume(f32),

    /// Sent by the input handler after the user picks a new genre
    /// from the live picker. The audio server replaces the current
    /// track list, clears the queue, and skips to a track from the
    /// new list. Ignored when `--tracks` is set (the user already
    /// chose a custom list).
    ChangeGenre(String),

    /// Updates the shared visual loading state; never changes audio playback.
    LoadPhaseChanged(LoadPhase),

    /// Quits gracefully.
    Quit,
}

/// The time to wait in between errors. Kept in lockstep with the prefetcher's
/// [`HTTP_TIMEOUT`](prefetcher::HTTP_TIMEOUT) so a single value drives
/// both the request budget and the retry backoff.
const TIMEOUT: Duration = prefetcher::HTTP_TIMEOUT;

/// The amount of songs to buffer up.
const BUFFER_SIZE: usize = 5;

/// Main struct responsible for queuing up & playing tracks.
// TODO: Consider refactoring [Player] from being stored in an [Arc], into containing many smaller [Arc]s.
// TODO: In other words, this would change the type from `Arc<Player>` to just `Player`.
// TODO:
// TODO: This is conflicting, since then it'd clone ~10 smaller [Arc]s
// TODO: every single time, which could be even worse than having an
// TODO: [Arc] of an [Arc] in some cases (Like with [Sink] & [Client]).
#[allow(
    clippy::arbitrary_source_item_ordering,
    reason = "Fields are grouped by concern (sink/state/inputs/persistence/io), not alphabetical. The data_dir / has_custom_tracks / list-as-RwLock additions follow the same convention."
)]
pub struct Player {
    /// [rodio]'s [`Sink`] which can control playback.
    pub sink: Sink,

    /// Loading status observed by the TUI independently of the audio message channel.
    pub load_phase: RwLock<LoadPhase>,

    /// The [`TrackInfo`] of the current track.
    /// This is [`None`] when lazylofi is buffering/loading.
    current: ArcSwapOption<tracks::Info>,

    /// The tracks, which is a [`VecDeque`] that holds
    /// *undecoded* [Track]s.
    ///
    /// This is populated specifically by the [Downloader].
    tracks: RwLock<VecDeque<tracks::Track>>,

    /// One downloaded track per inactive genre, ready for a genre switch.
    /// Shared with [`Prefetcher`] so picker-time downloads land in the same
    /// map the player consumes during `next` / `ChangeGenre`.
    prefetched: Arc<RwLock<HashMap<String, tracks::Track>>>,

    /// The standalone prefetcher that owns the shared map and the HTTP
    /// client. Held so post-pick re-warms have a handle without having
    /// to thread the prefetcher through every call site.
    prefetcher: Arc<Prefetcher>,

    /// The actual list of tracks to be played.
    ///
    /// Wrapped in an [`RwLock`] so that [`Messages::ChangeGenre`] can
    /// swap it out from a background task while [`Player::next`] and
    /// the [`Downloader`] keep reading from it.
    list: RwLock<List>,

    /// Path to the data directory, used to reload genre lists when
    /// [`Messages::ChangeGenre`] fires.
    pub data_dir: PathBuf,

    /// True if the user passed `--tracks`. When set, [`Messages::ChangeGenre`]
    /// is a no-op since the user already committed to a custom list.
    pub has_custom_tracks: bool,

    /// The initial volume level, as a percentage (0-100).
    volume: u16,

    /// The web client, which can contain a `UserAgent` & some
    /// settings that help lazylofi work more effectively.
    client: Client,

    /// The [`OutputStreamHandle`], which also can control some
    /// playback, is for now unused and is here just to keep it
    /// alive so the playback can function properly.
    _handle: OutputStreamHandle,

    /// The [`OutputStream`], which is just here to keep the playback
    /// alive and functioning.
    _stream: OutputStream,
}

// SAFETY: This is necessary because [OutputStream] does not implement [Send],
// due to some limitation with Android's Audio API.
// I'm pretty sure nobody will use lazylofi with android, so this is safe.
unsafe impl Send for Player {}

// SAFETY: See implementation for [Send].
unsafe impl Sync for Player {}

impl Player {
    /// This gets the output stream while also shutting up alsa with [libc].
    fn silent_get_output_stream() -> eyre::Result<(OutputStream, OutputStreamHandle)> {
        // Get the file descriptor to stderr from libc.
        extern "C" {
            static stderr: *mut libc::FILE;
        }

        // This is a bit of an ugly hack that basically just uses `libc` to redirect alsa's
        // output to `/dev/null` so that it wont be shoved down our throats.

        // The mode which to redirect terminal output with.
        let mode = CString::new("w")?;

        // First redirect to /dev/null, which basically silences alsa.
        let null = CString::new("/dev/null")?;

        // SAFETY: Simple enough to be impossible to fail. Hopefully.
        unsafe {
            freopen(null.as_ptr(), mode.as_ptr(), stderr);
        }

        // Make the OutputStream while stderr is still redirected to /dev/null.
        let (stream, handle) = OutputStream::try_default()?;

        // Redirect back to the current terminal, so that other output isn't silenced.
        let tty = CString::new("/dev/tty")?;

        // SAFETY: See the first call to `freopen`.
        unsafe {
            freopen(tty.as_ptr(), mode.as_ptr(), stderr);
        }

        Ok((stream, handle))
    }

    /// Just a shorthand for setting `current`.
    fn set_current(&self, info: tracks::Info) {
        self.current.store(Some(Arc::new(info)));
    }

    /// A shorthand for checking if `self.current` is [Some].
    pub fn current_exists(&self) -> bool {
        self.current.load().is_some()
    }

    /// Sets the volume of the sink, and also clamps the value to avoid negative/over 100% values.
    pub fn set_volume(&self, volume: f32) {
        self.sink.set_volume(volume.clamp(0.0, 1.0));
    }

    /// Initializes the entire player, including audio devices & sink.
    ///
    /// This also will load the track list & persistent volume.
    /// `genre` is the resolved genre (from `--genre` or the picker). When
    /// `args.tracks` is set, the genre is ignored.
    /// `prefetcher` provides the shared [`Client`], [`data_dir`](Prefetcher::data_dir),
    /// and the prefetched-track map. The picker constructs the prefetcher
    /// before showing the UI so first-track downloads already land in the
    /// map by the time the user picks.
    pub async fn new(
        args: &Args,
        genre: Option<String>,
        prefetcher: Arc<Prefetcher>,
    ) -> eyre::Result<Self> {
        // Load the volume file. When a genre is active, prefer that genre's
        // per-file volume. Otherwise (e.g. `--tracks`) fall back to the
        // global `volume.txt`.
        let volume = if let Some(genre) = &genre {
            PersistentGenreVolume::load(genre).await?
        } else {
            PersistentVolume::load().await?.inner()
        };

        // Load the track list using the data directory the prefetcher
        // already resolved (and ensured exists).
        let data_dir = prefetcher.data_dir.clone();
        let has_custom_tracks = prefetcher.has_custom_tracks;
        let client = prefetcher.client.clone();
        let list = List::load(&args.tracks, &genre, &data_dir).await?;

        // We should only shut up alsa forcefully if we really have to.
        let (_stream, handle) = if cfg!(target_os = "linux") && !args.alternate && !args.debug {
            Self::silent_get_output_stream()?
        } else {
            OutputStream::try_default()?
        };

        let sink = Sink::try_new(&handle)?;
        if args.paused {
            sink.pause();
        }

        let player = Self {
            load_phase: RwLock::new(LoadPhase::Scanning),
            tracks: RwLock::new(VecDeque::with_capacity(5)),
            prefetched: Arc::clone(&prefetcher.prefetched),
            prefetcher: Arc::clone(&prefetcher),
            current: ArcSwapOption::new(None),
            client,
            sink,
            volume,
            list: RwLock::new(list),
            data_dir,
            has_custom_tracks,
            _handle: handle,
            _stream,
        };

        Ok(player)
    }

    /// This will play the next track, as well as refilling the buffer in the background.
    ///
    /// This will also set `current` to the newly loaded song.
    pub async fn next(&self) -> eyre::Result<tracks::Decoded> {
        // The shared prefetched map is populated while the picker is
        // visible (see `play::play`), so the first `next` call after
        // `Init` consumes a downloaded track for the chosen genre
        // instead of doubling up with the `Downloader`. Falls through
        // to the queue / fresh-download path when the map is empty.
        let current_genre = self.list.read().await.name.clone();
        let prefetched_track = self.prefetched.write().await.remove(&current_genre);
        if let Some(track) = prefetched_track {
            let decoded = track.decode()?;
            self.set_current(decoded.info.clone());
            return Ok(decoded);
        }

        let queued = self.tracks.write().await.pop_front();

        let track = if let Some(track) = queued {
            track
        } else {
            // If the queue is completely empty, then fallback to simply getting a new track.
            // This is relevant particularly at the first song.

            // Serves as an indicator that the queue is "loading".
            // We're doing it here so that we don't get the "loading" display
            // for only a frame in the other case that the buffer is not empty.
            self.current.store(None);

            // Acquire the list read lock just long enough to call `random`,
            // and release it before doing anything else.
            self.list.read().await.random(&self.client).await?
        };

        let decoded = track.decode()?;

        // Set the current track.
        self.set_current(decoded.info.clone());

        Ok(decoded)
    }

    /// This basically just calls [`Player::next`], and then appends the new track to the player.
    ///
    /// This also notifies the background thread to get to work, and will send `TryAgain`
    /// if it fails. This functions purpose is to be called in the background, so that
    /// when the audio server recieves a `Next` signal it will still be able to respond to other
    /// signals while it's loading.
    ///
    /// This also sends the `NewSong` signal to `tx` apon successful completion.
    async fn handle_next(
        player: Arc<Self>,
        itx: Sender<()>,
        tx: Sender<Messages>,
    ) -> eyre::Result<()> {
        // Stop the sink.
        player.sink.stop();

        tx.send(Messages::LoadPhaseChanged(LoadPhase::Scanning))
            .await?;

        let track = player.next().await;

        match track {
            Ok(track) => {
                let freq = frequency_for(&track.info.name);
                tx.send(Messages::LoadPhaseChanged(LoadPhase::Tuning))
                    .await?;
                // Start playing the new track.
                player.sink.append(track.data);

                // Notify the background downloader that there's an empty spot
                // in the buffer.
                Downloader::notify(&itx).await?;

                tx.send(Messages::LoadPhaseChanged(LoadPhase::Locked { freq }))
                    .await?;

                // Notify the audio server that the next song has actually been downloaded.
                tx.send(Messages::NewSong).await?;
            }
            Err(error) => {
                // `List::random` reports both reqwest and filesystem errors
                // through `eyre`. A reqwest timeout is treated as a "try
                // again now" rather than waiting a full backoff cycle, since
                // timeouts usually mean the connection will succeed next
                // attempt.
                if !error::is_reqwest_timeout(&error) {
                    sleep(TIMEOUT).await;
                }

                tx.send(Messages::TryAgain).await?;
            }
        };

        Ok(())
    }

    /// This is the main "audio server".
    ///
    /// `rx` & `tx` are used to communicate with it, for example when to
    /// skip tracks or pause.
    ///
    /// This will also initialize a [Downloader] as well as an MPRIS server if enabled.
    #[allow(
        clippy::too_many_lines,
        reason = "Single match-on-message loop; splitting it would scatter the audio state machine. The added ChangeGenre handler pushed this past 100 lines."
    )]
    pub async fn play(
        player: Arc<Self>,
        tx: Sender<Messages>,
        mut rx: Receiver<Messages>,
    ) -> eyre::Result<()> {
        // Initialize the mpris player.
        //
        // We're initializing here, despite MPRIS being a "user interface",
        // since we need to be able to *actively* write new information to MPRIS
        // specifically when it occurs, unlike the UI which passively reads the
        // information each frame. Blame MPRIS, not me.
        #[cfg(feature = "mpris")]
        let mpris = mpris::Server::new(Arc::clone(&player), tx.clone())
            .await
            .inspect_err(|x| {
                if player.prefetcher.debug {
                    dbg!(x);
                }
            })?;

        // `itx` is used to notify the `Downloader` when it needs to download new tracks.
        let downloader = Downloader::new(Arc::clone(&player));
        let (itx, downloader) = downloader.start();

        // Start buffering tracks immediately.
        Downloader::notify(&itx).await?;

        // Pre-fetch the first track of every other genre without blocking playback.
        // The picker already ran `Prefetcher::warm_one` in the background for
        // the picked genre; this re-warm fills in any other genres the picker
        // didn't cover (custom genres, or all builtins if there was no
        // persisted genre at startup). Entries already in the shared map are
        // skipped, so this is a no-op when the picker warmed everything.
        let warm_player = Arc::clone(&player);
        let warm_prefetcher = Arc::clone(&player.prefetcher);
        task::spawn(async move {
            let current_genre = warm_player.list.read().await.name.clone();
            warm_prefetcher.warm_all(Some(&current_genre)).await;
        });

        // Set the initial sink volume to the one specified.
        player.set_volume(player.volume as f32 / 100.0);

        // Whether the last signal was a `NewSong`. This is helpful, since we
        // only want to autoplay if there hasn't been any manual intervention.
        //
        // In other words, this will be `true` after a new track has been fully
        // loaded  and it'll be `false` if a track is still currently loading.
        let mut new = false;

        loop {
            let clone = Arc::clone(&player);

            let msg = select! {
                biased;

                Some(x) = rx.recv() => x,
                // This future will finish only at the end of the current track.
                // The condition is a kind-of hack which gets around the quirks
                // of `sleep_until_end`.
                //
                // That's because `sleep_until_end` will return instantly if the sink
                // is uninitialized. That's why we put a check to make sure that the last
                // signal we got was `NewSong`, since we shouldn't start waiting for the
                // song to be over until it has actually started.
                //
                // It's also important to note that the condition is only checked at the
                // beginning of the loop, not throughout.
                Ok(()) = task::spawn_blocking(move || clone.sink.sleep_until_end()),
                        if new => Messages::Next,
            };

            match msg {
                Messages::LoadPhaseChanged(phase) => {
                    *player.load_phase.write().await = phase;
                }
                Messages::Next | Messages::Init | Messages::TryAgain => {
                    // We manually skipped, so we shouldn't actually wait for the song
                    // to be over until we recieve the `NewSong` signal.
                    new = false;

                    // This basically just prevents `Next` while a song is still currently loading.
                    if msg == Messages::Next && !player.current_exists() {
                        continue;
                    }

                    // Handle the rest of the signal in the background,
                    // as to not block the main audio server thread.
                    task::spawn(Self::handle_next(
                        Arc::clone(&player),
                        itx.clone(),
                        tx.clone(),
                    ));
                }
                Messages::Play => {
                    player.sink.play();

                    #[cfg(feature = "mpris")]
                    mpris.playback(PlaybackStatus::Playing).await?;
                }
                Messages::Pause => {
                    player.sink.pause();

                    #[cfg(feature = "mpris")]
                    mpris.playback(PlaybackStatus::Paused).await?;
                }
                Messages::PlayPause => {
                    if player.sink.is_paused() {
                        player.sink.play();
                    } else {
                        player.sink.pause();
                    }

                    #[cfg(feature = "mpris")]
                    mpris
                        .playback(mpris.player().playback_status().await?)
                        .await?;
                }
                Messages::ChangeVolume(change) => {
                    player.set_volume(player.sink.volume() + change);

                    // Persist the new volume in the background so we don't
                    // block the audio loop on disk I/O. When a genre is
                    // active we write to that genre's per-file volume;
                    // otherwise (e.g. `--tracks`) we fall back to the
                    // global `volume.txt`.
                    let current_volume = player.sink.volume();
                    let player_for_persist = Arc::clone(&player);
                    task::spawn(async move {
                        let result = if player_for_persist.has_custom_tracks {
                            PersistentVolume::save(current_volume).await
                        } else {
                            let genre_name = {
                                let guard = player_for_persist.list.read().await;
                                guard.name.clone()
                            };
                            PersistentGenreVolume::save(&genre_name, current_volume).await
                        };
                        if let Err(error) = result {
                            eprintln!("failed to persist volume: {error}");
                        }
                    });

                    #[cfg(feature = "mpris")]
                    mpris
                        .changed(vec![Property::Volume(player.sink.volume().into())])
                        .await?;
                }
                Messages::ChangeGenre(genre) => {
                    // `--tracks` overrides any live genre switch.
                    if !player.has_custom_tracks {
                        let player_clone = Arc::clone(&player);
                        let tx_clone = tx.clone();
                        let itx_clone = itx.clone();
                        let genre_clone = genre.clone();

                        task::spawn(async move {
                            let new_list = match List::load(
                                &None,
                                &Some(genre_clone.clone()),
                                &player_clone.data_dir,
                            )
                            .await
                            {
                                Ok(list) => list,
                                Err(error) => {
                                    eprintln!("failed to load genre '{genre_clone}': {error}");
                                    return;
                                }
                            };

                            // Replace the track list with the new genre.
                            *player_clone.list.write().await = new_list;

                            // Load and apply the new genre's volume.
                            // A failure here should not block the genre
                            // switch — the user can still adjust volume
                            // afterwards and we'll re-persist.
                            match PersistentGenreVolume::load(&genre_clone).await {
                                Ok(volume) => {
                                    player_clone.set_volume(volume as f32 / 100.0);
                                }
                                Err(error) => {
                                    eprintln!(
                                        "warning: failed to load volume for genre \
                                         '{genre_clone}': {error}"
                                    );
                                }
                            }

                            // Drain any tracks left over from the previous genre.
                            player_clone.tracks.write().await.clear();

                            let prefetched_track =
                                player_clone.prefetched.write().await.remove(&genre_clone);
                            if let Some(track) = prefetched_track {
                                player_clone.tracks.write().await.push_back(track);
                            }

                            // Stop the current track so `handle_next` can pick
                            // the next one from the new list.
                            player_clone.sink.stop();

                            if let Err(error) =
                                Self::handle_next(Arc::clone(&player_clone), itx_clone, tx_clone)
                                    .await
                            {
                                eprintln!("failed to skip after genre change: {error}");
                            }

                            // Re-warm every genre other than the one we just switched to.
                            let warm_prefetcher = Arc::clone(&player_clone.prefetcher);
                            let warm_new_genre = genre_clone.clone();
                            task::spawn(async move {
                                warm_prefetcher.warm_all(Some(&warm_new_genre)).await;
                            });
                        });
                    }
                }
                // This basically just continues, but more importantly, it'll re-evaluate
                // the select macro at the beginning of the loop.
                // See the top section to find out why this matters.
                Messages::NewSong => {
                    // We've recieved `NewSong`, so on the next loop iteration we'll
                    // begin waiting for the song to be over in order to autoplay.
                    new = true;

                    #[cfg(feature = "mpris")]
                    mpris
                        .changed(vec![
                            Property::Metadata(mpris.player().metadata().await?),
                            Property::PlaybackStatus(mpris.player().playback_status().await?),
                        ])
                        .await?;

                    continue;
                }
                Messages::Quit => break,
            }
        }

        downloader.abort();

        Ok(())
    }
}
