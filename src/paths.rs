//! Path helpers — single source for `~/.config/lazylofi` and
//! `~/.local/share/lazylofi` (or platform equivalents).
//!
//! All three helpers create the directory if it doesn't exist. Callers
//! that only need to *look up* a path should pass `dirs::config_dir()`
//! / `dirs::data_dir()` directly instead, since creating on every read
//! would litter the filesystem.

use std::path::PathBuf;

use eyre::eyre;
use tokio::fs;

/// Subdirectory of the user config tree lazylofi owns.
const CONFIG_SUBDIR: &str = "lazylofi";

/// Subdirectory of the user data tree lazylofi owns.
const DATA_SUBDIR: &str = "lazylofi";

/// Per-genre volume directory, relative to the config dir.
const VOLUME_SUBDIR: &str = "volume";

/// Resolves `~/.config/lazylofi/` (or platform equivalent) and ensures it
/// exists. Returns an error only if the user has no config directory at
/// all or creation fails.
pub async fn config_dir() -> eyre::Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| eyre!("could not resolve user config directory"))?
        .join(CONFIG_SUBDIR);
    fs::create_dir_all(&dir).await?;
    Ok(dir)
}

/// Resolves `~/.local/share/lazylofi/` (or platform equivalent) and ensures
/// it exists. Returns an error if the user has no data directory or
/// creation fails.
pub async fn data_dir() -> eyre::Result<PathBuf> {
    let dir = dirs::data_dir()
        .ok_or_else(|| eyre!("could not resolve user data directory"))?
        .join(DATA_SUBDIR);
    fs::create_dir_all(&dir).await?;
    Ok(dir)
}

/// Resolves `~/.config/lazylofi/volume/` (or platform equivalent) and
/// ensures it exists. The parent config dir is created too — a stock
/// install should never hit a missing parent, but the safeguard is
/// cheap.
pub async fn volume_dir() -> eyre::Result<PathBuf> {
    let dir = config_dir().await?.join(VOLUME_SUBDIR);
    fs::create_dir_all(&dir).await?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Smoke tests only: the `dirs` crate handles platform fallbacks
    // (XDG_CONFIG_HOME, HOME, registry, etc.), so we just confirm the
    // helpers don't error under the test environment and end with the
    // expected subdir name. Avoid mocking the dirs crate — its public
    // surface doesn't allow it.

    #[tokio::test]
    async fn config_dir_creates_lazylofi_subdir() {
        let dir = config_dir().await.unwrap();
        assert!(dir.ends_with(CONFIG_SUBDIR));
    }

    #[tokio::test]
    async fn volume_dir_lives_under_config_dir() {
        let config = config_dir().await.unwrap();
        let volume = volume_dir().await.unwrap();
        assert!(volume.starts_with(&config));
        assert!(volume.ends_with(VOLUME_SUBDIR));
    }
}
