//! The module containing all of the logic behind track lists,
//! as well as obtaining track names & downloading the raw mp3 data.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use eyre::{eyre, OptionExt as _};
use rand::Rng as _;
use reqwest::Client;
use tokio::fs;

use super::Track;

/// Represents a list of tracks that can be played.
///
/// See the [README](https://github.com/federicopinan/lazylofi?tab=readme-ov-file#custom-track-lists)
/// for more details about the format.
#[derive(Clone)]
pub struct List {
    /// The "name" of the list, usually derived from a filename.
    pub name: String,

    /// Just the raw file, but seperated by `/n` (newlines).
    /// `lines[0]` is the base, with the rest being tracks.
    lines: Vec<String>,
}

impl List {
    /// Gets the base URL of the [List].
    pub fn base(&self) -> &str {
        self.lines[0].trim()
    }

    /// Gets the name of a random track.
    fn random_name(&self) -> String {
        // We're getting from 1 here, since the base is at `self.lines[0]`.
        //
        // We're also not pre-trimming `self.lines` into `base` & `tracks` due to
        // how rust vectors work, sinceslow to drain only a single element from
        // the start, so it's faster to just keep it in & work around it.
        let random = rand::thread_rng().gen_range(1..self.lines.len());
        self.lines[random].clone()
    }

    /// Downloads a raw track, but doesn't decode it.
    async fn download(&self, track: &str, client: &Client) -> eyre::Result<Bytes> {
        // If the track has a protocol, then we should ignore the base for it.
        let url = if track.contains("://") {
            track.to_owned()
        } else {
            format!("{}{}", self.base(), track)
        };

        // Local files: bypass reqwest and read straight from disk. reqwest
        // does not implement the `file://` scheme by default and going
        // through it would add a needless layer of validation.
        if let Some(path) = url.strip_prefix("file://") {
            let bytes = fs::read(path).await?;
            return Ok(Bytes::from(bytes));
        }

        let response = client.get(url).send().await?;
        let data = response.bytes().await?;

        Ok(data)
    }

    /// Fetches and downloads a random track from the [List].
    pub async fn random(&self, client: &Client) -> eyre::Result<Track> {
        let name = self.random_name();
        let data = self.download(&name, client).await?;

        Ok(Track { name, data })
    }

    /// Parses text into a [List].
    pub fn new(name: &str, text: &str) -> Self {
        let lines: Vec<String> = text
            .split_ascii_whitespace()
            .map(ToOwned::to_owned)
            .collect();

        Self {
            lines,
            name: name.to_owned(),
        }
    }

    /// Builds a list by recursively scanning a local directory for `.mp3`
    /// files. The base becomes `file://<dir>/` and each track is its path
    /// relative to that directory so [`download()`](Self::download) can
    /// resolve them without going through reqwest.
    async fn from_directory(arg: &str) -> eyre::Result<Self> {
        let path = PathBuf::from(arg);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());

        let mut mpfiles: Vec<String> = Vec::new();
        let mut stack: Vec<PathBuf> = vec![canonical.clone()];

        while let Some(dir) = stack.pop() {
            let mut entries = fs::read_dir(&dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let entry_path = entry.path();
                let file_type = entry.file_type().await?;
                if file_type.is_dir() {
                    stack.push(entry_path);
                } else if file_type.is_file()
                    && entry_path.extension().and_then(|ext| ext.to_str()) == Some("mp3")
                {
                    if let Ok(rel) = entry_path.strip_prefix(&canonical) {
                        // `file://` URLs always use forward slashes, even on
                        // platforms where the OS prefers backslashes.
                        mpfiles.push(rel.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }

        if mpfiles.is_empty() {
            return Err(eyre!(
                "no .mp3 files found in '{}'; pass a .txt list or a directory that contains tracks",
                canonical.display()
            ));
        }

        let name = canonical
            .file_name()
            .and_then(|stem| stem.to_str())
            .unwrap_or("tracks")
            .to_owned();

        let base = format!("file://{}/", canonical.display());
        let mut lines = Vec::with_capacity(mpfiles.len() + 1);
        lines.push(base);
        lines.extend(mpfiles);

        Ok(Self { name, lines })
    }

    /// Reads a [List] from the filesystem using the CLI argument provided.
    ///
    /// When `tracks` is set, the custom list is loaded from disk:
    ///  - if the path is a directory, it's scanned recursively for `.mp3`
    ///    files and the base becomes `file://<dir>/`;
    ///  - if it's a file in the data directory (`<data_dir>/<name>.txt`),
    ///    that file is loaded;
    ///  - otherwise the argument is treated as a direct path to a `.txt`
    ///    list file.
    ///
    /// When `tracks` is `None`, `genre` selects one of the built-in lists
    /// embedded via `include_str!`, falling back to reading
    /// `<data_dir>/<genre>.txt` for user-added genres. If neither is set,
    /// the default `"lofi"` list is used.
    pub async fn load(
        tracks: &Option<String>,
        genre: &Option<String>,
        data_dir: &Path,
    ) -> eyre::Result<Self> {
        if let Some(arg) = tracks {
            // A directory means "scan my local music folder" — see
            // [`Self::from_directory`] for the resolution rules.
            let path = PathBuf::from(arg);
            if path.is_dir() {
                return Self::from_directory(arg).await;
            }

            // Check if the track is in ~/.local/share/lazylofi, in which case we'll load that.
            let candidate = dirs::data_dir()
                .ok_or_else(|| eyre!("could not resolve data directory"))?
                .join("lazylofi")
                .join(format!("{arg}.txt"));

            let name = if candidate.exists() {
                candidate
            } else {
                path.into()
            };

            let raw = fs::read_to_string(name.clone()).await?;

            let name = name
                .file_stem()
                .and_then(|x| x.to_str())
                .ok_or_eyre("invalid track path")?;

            Ok(Self::new(name, &raw))
        } else {
            let name = genre.as_deref().unwrap_or("lofi");
            let raw = match name {
                "lofi" => include_str!("../../data/lofi.txt").to_owned(),
                "synthwave" => include_str!("../../data/synthwave.txt").to_owned(),
                "jazz-lofi" => include_str!("../../data/jazz-lofi.txt").to_owned(),
                "ambient" => include_str!("../../data/ambient.txt").to_owned(),
                _ => {
                    let path = data_dir.join(format!("{name}.txt"));
                    fs::read_to_string(&path)
                        .await
                        .map_err(|err| eyre!("could not load genre '{name}': {err}"))?
                }
            };
            Ok(Self::new(name, &raw))
        }
    }
}
