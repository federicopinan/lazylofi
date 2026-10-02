# prefetch-pool — primer track instantáneo al cambiar de género

## Contexto

Hoy, cuando el usuario cambia de género con `g`, el handler `Messages::ChangeGenre` en `src/player.rs` drena el buffer de 5 tracks del género viejo y dispara un download fresco del primer track del nuevo género contra `archive.org`. Eso toma 2-5 segundos y el usuario ve "buffering".

El `Downloader` actual (`src/player/downloader.rs`) solo calienta el género **activo**. Los otros están fríos hasta que el usuario cambie.

## Objetivo

Tener **1 track pre-descargado por género "frío"** en memoria. Al cambiar, el primer track sale del pool — instantáneo. Después, el buffer normal de 5 toma el relevo con el `Downloader` que ya existe.

## Decisiones de producto

- **Pool de 1 track por género frío**: suficiente para el primer track. El `Downloader` actual llena los 5 siguientes del nuevo género activo.
- **Géneros a pre-fetchear**: los 4 built-ins (`lofi`, `synthwave`, `jazz-lofi`, `ambient`) + cualquier `*.txt` adicional en `~/.local/share/lazylofi/` (misma fuente que el picker).
- **Cuándo se rellena el pool**:
  - Al arrancar (`Player::play` → después de iniciar el `Downloader`).
  - Después de cada `ChangeGenre` exitoso.
- **Cuándo NO se pre-cachea**: cuando `has_custom_tracks` es true (custom list del usuario, no hay "otros géneros").
- **Exclusión**: nunca pre-fetchear el género que está sonando ahora (ya lo cubre el `Downloader`).
- **Vida del pool**: en memoria, muere con el proceso. Sin persistencia en disco (eso sería la opción B).
- **Memoria**: ~30 MB extra pico (3-4 tracks × ~7 MB promedio). Aceptable para desktops modernos.
- **Cap defensivo**: si el directorio tiene más de 8 archivos custom, loguear warning y capear a 8 para no abusar de la red ni de la RAM.
- **Errores**: si la carga o el download de un género frío falla, loguear y seguir. No romper el switch del usuario.
- **Idempotencia**: si el pool ya tiene track para un género, skip (no re-bajar).

## Arquitectura

```text
Player (Arc)
├── tracks: RwLock<VecDeque<Track>>            # buffer del género actual (5)
├── prefetched: RwLock<HashMap<String, Track>> # NUEVO: 1 track por género frío
├── list: RwLock<List>                         # género activo
└── ...

Downloader (existente)   # llena buffer del género activo (5 tracks)
Prefetcher (nuevo)        # llena prefetched con 1 track por género frío
```

### Flow de `ChangeGenre` (modificado)

```rust
Messages::ChangeGenre(genre) => {
    if !player.has_custom_tracks {
        spawn(async move {
            // 1. Cargar nueva lista
            let new_list = List::load(&None, &Some(genre_clone), &data_dir).await?;
            *player.list.write().await = new_list;

            // 2. Volumen del nuevo género
            player.set_volume(load_genre_volume(&genre_clone).await);

            // 3. Drenar buffer del género viejo
            player.tracks.write().await.clear();

            // 4. NUEVO: Pop del pool si hay track pre-cargado
            let prefetched = player.prefetched.write().await.remove(&genre_clone);

            // 5. Stop sink + handle_next (usa el buffer si hay algo)
            player.sink.stop();
            if let Some(t) = prefetched {
                player.tracks.write().await.push_back(t);
            }
            Self::handle_next(...).await;

            // 6. NUEVO: Re-warm los otros géneros en background
            task::spawn(Prefetcher::warm_all(player.clone()));
        });
    }
}
```

### `Prefetcher::warm_all` (nuevo)

```rust
pub async fn warm_all(player: Arc<Player>) {
    let current_genre = player.list.read().await.name.clone();
    let genres = collect_genres(&player.data_dir);   // built-ins + data_dir/*.txt

    for genre in genres {
        if genre == current_genre { continue; }
        if player.prefetched.read().await.contains_key(&genre) { continue; }

        match List::load(&None, &Some(genre.clone()), &player.data_dir).await {
            Ok(list) => match list.random(&player.client).await {
                Ok(track) => { player.prefetched.write().await.insert(genre, track); }
                Err(e) => eprintln!("prefetch '{genre}' download failed: {e}"),
            },
            Err(e) => eprintln!("prefetch '{genre}' load failed: {e}"),
        }
    }
}
```

## Tareas

### T1 — Campo `prefetched` en `Player`

En `src/player.rs`:
- Importar `std::collections::HashMap`.
- Agregar campo `prefetched: RwLock<HashMap<String, Track>>` al `Player`.
- Inicializar en `Player::new` con `RwLock::new(HashMap::new())`.

### T2 — Módulo `src/player/prefetcher.rs`

Archivo nuevo con:

```rust
use std::{collections::HashSet, path::Path, sync::Arc};
use eyre::Result;
use tokio::task;

use super::Player;
use crate::tracks::list::List;

pub struct Prefetcher;

impl Prefetcher {
    /// Pre-fetch 1 track for every genre except the currently active one.
    pub async fn warm_all(player: Arc<Player>) {
        // 1. Get current genre name
        let current = player.list.read().await.name.clone();

        // 2. Discover available genres (built-ins + custom .txt files).
        let genres = collect_genres(&player.data_dir);
        let genres: Vec<String> = genres
            .into_iter()
            .filter(|g| g != &current)
            .take(MAX_PREFETCH)   // cap defensivo
            .collect();

        for genre in genres {
            // 3. Skip si ya hay algo en el pool.
            if player.prefetched.read().await.contains_key(&genre) { continue; }

            // 4. Cargar lista del género y bajar 1 track.
            let load_result = List::load(&None, &Some(genre.clone()), &player.data_dir).await;
            match load_result {
                Ok(list) => match list.random(&player.client).await {
                    Ok(track) => {
                        player.prefetched.write().await.insert(genre, track);
                    }
                    Err(e) => eprintln!("prefetch '{genre}' download failed: {e}"),
                },
                Err(e) => eprintln!("prefetch '{genre}' load failed: {e}"),
            }
        }
    }
}

/// Built-in genre names.
const BUILTIN_GENRES: &[&str] = &["lofi", "synthwave", "jazz-lofi", "ambient"];

/// Cap defensivo: máximo de géneros a pre-fetchear.
const MAX_PREFETCH: usize = 8;

/// Enumera los géneros disponibles: 4 built-ins + cualquier .txt en data_dir.
///
/// Excluye `micropop.txt` (custom list de ejemplo), archivos ocultos, y
/// duplicados.
pub fn collect_genres(data_dir: &Path) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();

    for builtin in BUILTIN_GENRES {
        if seen.insert((*builtin).to_owned()) {
            out.push((*builtin).to_owned());
        }
    }

    // Leer custom .txt files del data_dir.
    if let Ok(entries) = std::fs::read_dir(data_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() { continue; }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue; };
            if !name.ends_with(".txt") { continue; }
            if name.starts_with('.') { continue; }
            if name == "micropop.txt" { continue; }

            let stem = name.trim_end_matches(".txt").to_owned();
            if seen.insert(stem.clone()) {
                out.push(stem);
            }
        }
    }

    out
}
```

### T3 — Disparar warm inicial en `Player::play`

Después de `Downloader::notify(&itx).await?` (línea ~368):

```rust
// Pre-fetch first track of every other genre.
task::spawn(Prefetcher::warm_all(Arc::clone(&player)));
```

Agregar `pub mod prefetcher;` en `src/player.rs` junto a los otros `pub mod`.

### T4 — Modificar handler `ChangeGenre`

En `src/player.rs` `Messages::ChangeGenre(genre)` (líneas ~475-532), dentro del `task::spawn`:

1. Después de drenar `player_clone.tracks.write().await.clear();`:
   ```rust
   let prefetched_track = player_clone.prefetched.write().await.remove(&genre_clone);
   if let Some(track) = prefetched_track {
       player_clone.tracks.write().await.push_back(track);
   }
   ```
2. El `handle_next` sigue igual — si el buffer tiene algo lo usa, si no descarga.
3. Después del `handle_next`, agregar:
   ```rust
   task::spawn(Prefetcher::warm_all(Arc::clone(&player_clone)));
   ```

### T5 — `collect_genres` para el picker

**Opcional pero recomendado**: extraer la enumeración de géneros a un helper reutilizable. Si `src/player/ui/picker.rs` ya tiene una función similar, reemplazar su uso por `prefetcher::collect_genres`. Si no, dejarla duplicada por ahora y abrir un follow-up.

### T6 — Verificación

- `cargo check`: 0 errores.
- `cargo clippy --all-features`: 0 warnings nuevos en archivos tocados. Si aparecen, agregar `#[allow(...)]` LOCAL con razón clara (NO allows globales).
- `cargo build --release`: binario OK.
- Smoke test manual: arrancar con `--genre lofi`, esperar unos segundos a que se llene el pool, después cambiar a `synthwave` con `g` → primer track suena instantáneo. Repetir con los demás géneros.

### T7 — Notas de implementación

- El `Player` tiene `unsafe impl Send` y `unsafe impl Sync` por el `OutputStream` de rodio. Los nuevos campos (`RwLock<HashMap<...>>`) son `Send + Sync` por defecto (estándar).
- No hace falta tocar `mpris.rs` ni `tracks.rs` ni `play.rs` ni `main.rs`. El cambio es contenido en `player.rs` + módulo nuevo.
- `cargo clippy` se queja mucho en este repo (lints estrictos heredados). Aplicar el patrón de los PRs anteriores: `#[allow(...)]` local con razón inline.

## Archivos a tocar

- `src/player.rs` — agregar campo `prefetched`, `pub mod prefetcher;`, modificar handler `ChangeGenre`, agregar `Prefetcher::warm_all` spawn en startup.
- `src/player/prefetcher.rs` (nuevo) — `Prefetcher::warm_all` + `collect_genres`.
- `src/player/ui/picker.rs` (opcional) — si querés deduplicar `collect_genres`.

## Out of scope

- Caché en disco (eso sería la opción B, otro feature doc).
- Pool de N tracks por género frío (1 es suficiente).
- Pre-fetchear también para `--tracks` (no aplica).
- LRU eviction (con cap de 8 y 1 track por género no hace falta).
- Migrar UI a ratatui (ya está hecho).
- Persistir el pool entre sesiones (sin disk cache no es posible).

## Riesgos

- **`--tracks` activo**: no pre-fetcheamos nada (correcto). `g` queda deshabilitado igual que hoy.
- **Muchos custom `.txt`**: cap defensivo `MAX_PREFETCH = 8` para no abusar.
- **Race conditions**: si el usuario cambia 2 veces rápido, `warm_all` puede solaparse. La idempotencia de `contains_key` y la atomicidad de `RwLock::insert` lo cubren. No es un problema funcional.
- **Memoria**: ~30 MB con 4 built-ins. ~100 MB si hay 10 custom. Aceptable para desktop; cap protege.
- **Network usage**: el warm inicial hace 3-4 downloads extra en background (no bloquea el playback). Si el usuario tiene red medida, esto es un costo. Documentar en README cuando se haga la sección.

## Estado

- [x] Plan escrito
- [x] T1 — Campo `prefetched` en Player
- [x] T2 — Módulo `prefetcher.rs`
- [x] T3 — Warm inicial en `Player::play`
- [x] T4 — Modificar `ChangeGenre` handler
- [ ] T5 — (Opcional) deduplicar `collect_genres` — diferido, ver nota
- [x] T6 — Verificación (`cargo check`, `clippy --all-features`, `cargo build --release --all-features` — todos OK)
- [ ] T7 — Smoke test manual (paso a user)

### Notas de implementación

- T5 se dejó pendiente a propósito: el picker trata los errores de lectura de
  forma distinta y deduplicarlo alteraría ese comportamiento. La duplicación
  es pequeña (~10 líneas) y se puede abrir como follow-up si crece.
- Verificación final: `cargo check` solo emite el warning pre-existente
  (`Messages::Play` nunca usado); `cargo clippy --all-features` no tiene
  warnings nuevos en `player.rs` ni en `prefetcher.rs`; `cargo build
  --release --all-features` produce `target/release/lazylofi`.
