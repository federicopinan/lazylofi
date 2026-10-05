# lazylofi

![lazylofi header](media/header.jpeg)

A tiny Rust TUI music player for **lofi**, **synthwave**, **jazz lofi**, and **ambient**.

lazylofi does one thing and tries to do it well: play music from your terminal.
No albums, no ads, no playlists you can't escape — just a vintage FM-style radio
that streams curated tracks (or your own local files) while you work.

It ships with four built-in genres, a live genre picker, a per-track "frequency"
that turns every track into its own little radio station, and an animated ocean
gently rolling at the bottom of the screen.

## Features

- **Vintage FM-style UI** — a frequency display, an analog dial, and an animated
  needle that sweeps while loading.
- **Deterministic per-track frequency** — each track hashes to a unique station
  between 88.0 and 108.0 MHz, so the same track always lands on the same
  frequency. Each song feels like its own radio station.
- **Visual loading feedback** — three states (`scanning stations...`,
  `tuning in...`, `locked · 88.5 MHz`) with an ASCII glyph spinner. No more
  wondering if the app is hung.
- **Animated ocean band** — two sine-driven rows of `~` and `░▒▓` scroll at the
  bottom of every screen, including `--minimalist`.
- **Live genre picker** — press `g` mid-playback to swap genres without
  restarting. The current track skips and the new one replaces the queue.
- **Custom track lists** — point `--tracks` at a `.txt` of URLs OR at a local
  directory and lazylofi will recursively scan it for `.mp3` files.
- **Per-genre volume** — each genre remembers its own volume, so
  `synthwave` can stay loud while `ambient` stays quiet.
- **Buffered playback** — five songs are buffered at a time so a slow track
  fetch doesn't interrupt the current song.
- **MPRIS support** (optional feature) — integration with Linux media keys and
  status indicators.
- **Cross-platform** — Linux, macOS, and Windows.

## Why?

Most terminal music players either want you to maintain a music library or
pull from a streaming service that eventually breaks. lazylofi sidesteps both:
it ships with hand-curated tracks that actually work, a transparent way to drop
in your own, and enough buffering to survive a flaky network.

The goal is simplicity: no albums, no playlists you can't escape, no ads. Just
music.

## Installing

### Homebrew (macOS, Linux)

```sh
brew install federicopinan/tap/lazylofi
```

Pulls the formula from [`federicopinan/homebrew-tap`](https://github.com/federicopinan/homebrew-tap)
and builds from source. On Linux the formula pulls in `alsa-lib` and `openssl@3`
as build deps so you don't need them on the system path.

### Cargo (recommended)

You'll need **Rust 1.74.0 or newer**.

- **macOS** and **Windows**: no extra dependencies.
- **Linux**: you'll need `openssl`, `alsa-lib`, and their `-dev` headers.

```sh
# Arch / Manjaro
sudo pacman -S alsa-lib openssl pkgconf

# Debian / Ubuntu
sudo apt install libasound2-dev libssl-dev pkg-config

# Fedora
sudo dnf install alsa-lib-devel openssl-devel pkgconf-pkg-config
```

If you're using PulseAudio on Linux, also install `pulseaudio-alsa` so the
default sink lines up with what your desktop mixer expects.

```sh
cargo install lazylofi

# With MPRIS support (Linux media keys, status indicators)
cargo install lazylofi --features mpris
```

Make sure `$HOME/.cargo/bin` is on your `$PATH`.

### Pre-built binaries

Grab a binary for your platform from the
[latest release](https://github.com/federicopinan/lazylofi/releases/latest) if you'd
rather skip the build.

### AUR

AUR packaging is planned but not yet published. In the meantime, install from
cargo or a release binary.

### From source

```sh
git clone https://github.com/federicopinan/lazylofi
cd lazylofi
cargo build --release
./target/release/lazylofi
```

## Quick start

```sh
# Open the genre picker (default).
lazylofi

# Skip the picker and start playing a built-in genre.
lazylofi --genre synthwave
lazylofi -g jazz-lofi
lazylofi -g ambient

# Play your own local music folder.
lazylofi --tracks ~/Music/lofi

# Play a custom .txt list of URLs.
lazylofi --tracks mi_lista

# Start paused (great for tweaking layout before audio starts).
lazylofi --paused

# Minimalist mode: hide the genre/track info, keep frequency + dial + progress.
lazylofi --minimalist
```

## Command-line flags

| Flag                  | Description                                                                      |
|----------------------|--------------------------------------------------------------------------------|
| `-a`, `--alternate`  | Use the alternate terminal screen.                                             |
| `-m`, `--minimalist` | Hide the bottom controls and the genre/track rows.                             |
| `-p`, `--paused`     | Start paused instead of playing immediately.                                   |
| `-d`, `--debug`      | Verbose diagnostic output (prefetcher failures, mpris init errors).            |
| `-w`, `--width`      | Width of the player (0–32). Default `3`.                                       |
| `-t`, `--tracks`     | Custom track list: path to a `.txt`, name of a `.txt` in the data directory, **or a directory of `.mp3` files**. Short alias: `-l`, `--list`. |
| `-g`, `--genre`      | Skip the picker and play a built-in genre (`lofi`, `synthwave`, `jazz-lofi`, `ambient`). |

## Controls

### While playing

| Key      | Function                  |
|----------|---------------------------|
| `s`      | Skip to next song         |
| `p`      | Toggle play / pause       |
| `+` / `-`| Volume up / down          |
| `g`      | Open the live genre picker|
| `q` / `Esc` | Quit                   |

Pressing `g` swaps genres without restarting: the current track skips, the
queue is drained, and the first track from the new genre comes in.
When you launched with `--tracks`, the `g` slot is disabled because there is no
genre to swap to.

### While the picker is open

| Key           | Function                          |
|---------------|----------------------------------|
| `↑` / `k`     | Move selection up                |
| `↓` / `j`     | Move selection down              |
| `Enter`       | Confirm selection                |
| `q` / `Esc`   | Close the picker (no change)     |

## Custom Track Lists

`--tracks` is the most flexible way to feed lazylofi. The argument can be
**three** different things:

1. **A path to a `.txt` file** (anywhere on disk).
2. **The name of a `.txt` file in the data directory** — on Linux that's
   `~/.local/share/lazylofi/<name>.txt`.
3. **A directory of `.mp3` files** — lazylofi scans it recursively and plays
   everything it finds. No `.txt` required.

```sh
# Directory: every .mp3 underneath, recursively.
lazylofi --tracks ~/Music/lofi

# Local .txt with full URLs (no base/path juggling needed).
lazylofi --tracks ~/lists/jazz.txt

# A list file under the data directory (omit the .txt extension).
lazylofi --tracks mi_lista   # → ~/.local/share/lazylofi/mi_lista.txt
```

When `--tracks` points at a directory, lazylofi prints a clear error if no MP3s
are found:

```
Error: no .mp3 files found in '/path/to/empty'; pass a .txt list or a
directory that contains tracks
```

### The `.txt` format

A list file is plain text:

- **Line 1** is the base URL (usually a trailing-slash directory on a server).
- **Lines 2+** are track names. If a line contains `://` it's used as-is
  (full URL); otherwise the base URL is prepended.

The shipped `data/<genre>.txt` files use **full URLs** for every track, so the
base line is never consulted. That pattern is the easiest to maintain:

```txt
https://archive.org/download/jamendo-506817/01-1995378-The_Mountain-Chill%20LoFi%20Hip-Hop.mp3
https://archive.org/download/DWK243/Jazz_One_-_03_-_Barefoot_Ballet.mp3
https://example.com/path/to/track.mp3
```

If you'd rather use relative paths, give the base URL a trailing slash and put
just the filenames below it:

```txt
https://archive.org/download/jamendo-506817/
01-1995378-The_Mountain-Chill%20LoFi%20Hip-Hop.mp3
https://archive.org/download/DWK243/Jazz_One_-_03_-_Barefoot_Ballet.mp3
```

`file://` URLs work too, but pointing `--tracks` at a directory is usually
simpler.

## Configuration

lazylofi keeps two small pieces of state in `~/.config/lazylofi/`:

- **`genre.txt`** — the last genre you picked (or passed via `--genre`).
  Resuming is automatic: next launch with no arguments skips the picker and
  goes straight to that genre. `--tracks` launches don't overwrite this file,
  so a custom-list session never displaces your real genre.
- **`volume/<genre>.txt`** — the volume (integer 0–100) for each genre. The
  global `volume.txt` is the fallback for `--tracks` sessions where there's no
  genre to key on.

Reset either by deleting the file:

```sh
rm ~/.config/lazylofi/genre.txt                 # forget the saved genre
rm ~/.config/lazylofi/volume/synthwave.txt      # reset one genre's volume
```

The action bar shows the active genre as a `[genre]` prefix, or `[tracks]`
when you launched with `--tracks`.

## Genre Lists

lazylofi ships with four built-in genres, all backed by hand-curated URLs on
the [Internet Archive](https://archive.org/):

- **lofi** — chillhop podcast archive, Jamendo, Jazz One.
- **synthwave** — `synthwave-artifacts-retro-wave-2020`,
  `the-gates-synthspace-anthology`, Jamendo.
- **jazz lofi** — Jazz One (`DWK243`, `DWK284`), Jamendo, plus a couple of
  full mixes.
- **ambient** — `swt-ambientcollection`, `ambient-time-py7kvd`.

Files live in `data/<genre>.txt` and are baked into the binary at compile
time. If a track 404s, edit the corresponding `.txt` and replace the URL.

You can also drop your own `.txt` files into the data directory
(`~/.local/share/lazylofi/`) — they'll show up in the picker automatically.
See [Custom Track Lists](#custom-track-lists) for the format.

## The `scrape` subcommand

Dumps every link matching a given extension under a URL directory listing —
useful for finding new MP3s to add to your lists.

```sh
lazylofi scrape --base https://archive.org/download/some-collection/
lazylofi scrape --base https://example.com/files/ --include-full=false
```

The `--base` URL must contain `://`. Empty or missing values are rejected with
a clear error.

## Disclaimer

All audio streamed by lazylofi comes from the
[Internet Archive](https://archive.org/). Tracks are typically CC-licensed or
public domain; please respect each item's individual license on `archive.org`
if you plan anything beyond personal use.

## License

lazylofi is released under the **MIT License**. See [`LICENSE`](LICENSE) for
the full text.

```
MIT License
Copyright (c) 2025 Federico
```

## Credits

- [Internet Archive](https://archive.org/) — hosts every built-in track.
- Chillhop, Jamendo, Jazz One, and the other artists whose music makes this
  project worth shipping.