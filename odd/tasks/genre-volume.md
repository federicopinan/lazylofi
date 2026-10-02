# genre-volume — persistir volumen por género

## Contexto

Hoy `PersistentVolume` guarda un único volumen global en
`~/.config/lazylofi/volume.txt`. Cada vez que cambiás de género con `g`, el
volumen se mantiene — está bien, pero lo interesante sería que cada género
recuerde **su propio volumen**. Así, si subís el volumen para synthwave
porque te gusta más alto, y después pasás a ambient que necesita más
quietud, el cambio queda guardado por separado.

Feature: cada género tiene su propio volumen. Se persiste en
`~/.config/lazylofi/volume/<genre>.txt`. El `volume.txt` global sigue
existiendo como fallback para el caso `--tracks` (sin género).

## Decisiones de diseño

- **Storage**: `~/.config/lazylofi/volume/<genre>.txt`, una línea con
  el número (0-100). Default 100 si no existe.
- **Backward compat**: el `volume.txt` global NO se deprecá. Se usa
  cuando el usuario corrió con `--tracks` (no hay género).
- **Carga**: en `Player::new`, si hay género, usar
  `PersistentGenreVolume::load(genre)`; si no, `PersistentVolume::load()`.
- **Persistencia en ChangeVolume**:
  - Si hay género activo (`player.list.read().await.name`):
    `PersistentGenreVolume::save(&genre, volume)`.
  - Si no (caso `--tracks`): `PersistentVolume::save(volume)` (como hoy).
- **Cambio de género**: el handler `ChangeGenre` carga el volumen del nuevo
  género y lo aplica al sink.
- **Persistencia al cerrar**: además de seguir guardando el `volume.txt`
  global en `play::play()`, también guardar el del género activo.

## Tareas

### T1 — Nuevo `PersistentGenreVolume` en `src/play.rs`

```rust
pub struct PersistentGenreVolume;

impl PersistentGenreVolume {
    /// Returns `~/.config/lazylofi/volume/` (creating it if needed).
    async fn dir() -> eyre::Result<PathBuf> { /* ... */ }

    /// Returns the path for a given genre: `~/.config/lazylofi/volume/<genre>.txt`.
    async fn path(genre: &str) -> eyre::Result<PathBuf> { /* ... */ }

    /// Loads the volume for a genre. Returns 100 (default) if the file
    /// doesn't exist. Returns Err if the file exists but is malformed.
    pub async fn load(genre: &str) -> eyre::Result<u16> { /* ... */ }

    /// Saves `volume` (as percentage 0-100) to the genre's file.
    pub async fn save(genre: &str, volume: f32) -> eyre::Result<()> { /* ... */ }
}
```

Validar el nombre del género antes de usarlo como parte del path
(no `..`, no `/`, no caracteres raros). Si el nombre es inválido, retornar
error. Los 4 built-ins (`lofi`, `synthwave`, `jazz-lofi`, `ambient`) son
seguros por construcción.

### T2 — Wirearlo en `Player::new`

En `src/player.rs::Player::new`, donde se carga el volumen:

```rust
let volume = if let Some(genre) = &genre {
    PersistentGenreVolume::load(genre).await?
} else {
    PersistentVolume::load().await?.inner
};
```

Convertir `PersistentVolume` a solo `u16` o mantener la abstracción.
Decisión: cambiar el campo `volume: PersistentVolume` a `volume: u16` en
el `Player` para simplificar. O agregar un wrapper que sea "either kind".
**Decisión**: cambiar a `volume: u16` para mantener simple.

`set_volume`, `play`, etc., usan `player.volume.float()` → adaptar a
`player.volume as f32 / 100.0` o agregar un helper.

### T3 — Wirearlo en `ChangeGenre` handler

En `src/player.rs::Player::play`, después de reemplazar la lista:

```rust
// Cargar volumen del nuevo género y aplicarlo.
let new_volume = PersistentGenreVolume::load(&genre_clone)
    .await
    .unwrap_or(100);
player_clone.set_volume(new_volume as f32 / 100.0);
```

### T4 — Wirearlo en `ChangeVolume` handler

En el handler `Messages::ChangeVolume` del loop de `Player::play`:

```rust
Messages::ChangeVolume(change) => {
    player.set_volume(player.sink.volume() + change);

    // Persistir el volumen.
    let genre_name = player.list.read().await.name.clone();
    let current_volume = player.sink.volume();
    let player_clone = Arc::clone(&player);
    task::spawn(async move {
        let result = if genre_name.is_empty() || genre_name == "tracks" {
            // Caso --tracks o similar: usar global.
            PersistentVolume::save(current_volume).await
        } else {
            PersistentGenreVolume::save(&genre_name, current_volume).await
        };
        if let Err(error) = result {
            eprintln!("failed to persist volume: {error}");
        }
    });

    #[cfg(feature = "mpris")]
    mpris.changed(...).await?;
}
```

**Decisión sobre el nombre del archivo en --tracks**: si `--tracks` está
activo, `list.name` es el nombre del archivo (ej: `mi_lista`). Para no
crear archivos con nombres arbitrarios del usuario, cuando el nombre
contiene caracteres no-seguros o es "tracks", usar el global.

**Más simple**: si `has_custom_tracks` está activo, siempre usar global.
Si no, usar el archivo del género.

```rust
let result = if player.has_custom_tracks {
    PersistentVolume::save(current_volume).await
} else {
    let genre_name = player.list.read().await.name.clone();
    PersistentGenreVolume::save(&genre_name, current_volume).await
};
```

### T5 — Verificación

- `cargo check`: 0 errores.
- `cargo clippy --all-features`: 0 warnings nuevos en código tocado.
- `cargo build --release`: binario OK.

### T6 — Actualizar README

Agregar nota en la sección de "Persisted Genre" o nueva mini-sección
"Per-Genre Volume":

> Each genre remembers its own volume. Stored at
> `~/.config/lazylofi/volume/<genre>.txt`. Delete the file to reset to 100%.
> When you launch with `--tracks`, the global `~/.config/lazylofi/volume.txt`
> is used instead.

## Riesgos / decisiones

- **Persistencia por cada `ChangeVolume`**: eso significa escribir al disco
  en cada `+`, `-`, flecha arriba/abajo. En SSD moderno es instantáneo,
  pero podría causar lag en discos lentos. Workaround: rate-limit la
  escritura (no persistir más de una vez cada 500ms). **Decisión**:
  implementar versión simple primero, agregar rate-limit si causa
  problemas.
- **Carácter raro en nombre de género**: si el usuario pasa `--genre
  "foo/bar"`, el path sería `volume/foo/bar.txt`. Hay que validar o
  sanitizar el nombre. Para los 4 built-ins no es problema, pero si el
  user pone algo raro en un `.txt` custom y lo carga vía picker, podría
  romper. **Decisión**: validar (rechazar `..`, `/`, etc.).
- **Persistencia al cerrar**: hoy `play::play()` hace
  `PersistentVolume::save(player.sink.volume())`. Mantener eso
  (compatible hacia atrás) Y agregar la persistencia per-genre si el
  género está activo.

## Out of scope

- Migrar UI del player a ratatui (sigue pendiente, feature doc aparte).
- Historial de cambios de género.
- Crossfade entre tracks.

## Estado

- [x] T1 — PersistentGenreVolume módulo
- [x] T2 — Player::new carga volumen por género
- [x] T3 — ChangeGenre aplica volumen del nuevo género
- [x] T4 — ChangeVolume persiste al archivo del género
- [x] T5 — Verificación
- [x] T6 — README