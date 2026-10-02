# lazylofi — fork de lowfi con catálogo multi-género

## Contexto

Fork local de `lowfi` (Rust TUI player, MIT, single-purpose) renombrado a
**lazylofi**, en línea con el patrón "lazy TUI tools" de Linux (lazygit,
lazydocker). Se conserva toda la base de audio/TUI/descarga; se cambian:

1. Nombre del paquete y binario (`lowfi` → `lazylofi`).
2. Catálogo por género: **lofi**, **synthwave**, **jazz lofi**, **ambient**.
3. Selector de género: `--genre <name>` flag + picker interactivo al
   iniciar cuando no se pasa `--tracks` ni `--genre`.
4. Limpieza de artefactos del repo original (`.git/`, `.github/`, `LICENSE`,
   `.atl/`, `.pi/`).

Fuentes de catálogo elegidas por el usuario: **Internet Archive + Lofi
Girl**. Lofi Girl aporta lofi/jazz lofi; Internet Archive aporta synthwave
y ambient (música libre, descarga directa con `archive.org/download/<id>/`).

## Decisiones de producto (confirmadas con el usuario)

- **Catálogo**: Internet Archive + Lofi Girl. Mantenemos la lista Lofi Girl
  original (`data/lofigirl.txt` → `data/lofi.txt`) y creamos 3 listas nuevas
  con URLs reales de archive.org. Si el path exacto de una URL falla al
  primer request, el downloader ya tiene retry — el usuario puede editar
  `data/<genre>.txt` para ampliar/curar.
- **Selector**: picker interactivo al iniciar (flechas ↑/↓ + Enter) cuando no
  hay `--genre` ni `--tracks`. Mantiene el flow suckless del original sin
  obligar al usuario a conocer el nombre del género.
- **Flag `--genre`**: atajo para saltar el picker.
  `lazylofi --genre synthwave` carga `data/synthwave.txt`.
- **Compatibilidad `--tracks`**: intacta (custom lists del usuario en
  `~/.local/share/lazylofi/`).

## Tareas

### T1 — Limpieza de artefactos del repo original — DONE

Eliminados: `.git/`, `.github/`, `.atl/`, `.pi/`, `LICENSE`,
`data/sample.txt`. `README.md` y `data/micropop.txt` se mantuvieron
(reescritos/conservados en T7/T6 respectivamente).

### T2 — Renombrar paquete en `Cargo.toml` — DONE

- `name`: `lowfi` → `lazylofi`
- `description`: "A lazy TUI player for lofi, synthwave, jazz lofi, and
  ambient music."
- `license`: `MIT` (la misma que vamos a usar)
- `homepage`/`repository`/`documentation`: `https://github.com/talwat/lazylofi`
  (placeholder; ajustar cuando se defina el repo destino)
- `keywords`: `lazylofi, lofi, synthwave, ambient, jazz, music, tui`
- `version`, `edition`, `features`, `dependencies`: intactos.

### T3 — Rebranding hardcoded en código fuente — DONE

- `src/main.rs`: header doc-comment, doc-comment del struct `Args`,
  path `~/.local/share/lazylofi`.
- `src/play.rs`: `dirs::config_dir().join("lowfi")` → `join("lazylofi")`.
- `src/tracks/list.rs`: `dirs::data_dir().join("lowfi")` → `join("lazylofi")`.
- `src/player.rs`: 4 doc-comments donde decía "lowfi" ahora dicen
  "lazylofi".
- `src/player/mpris.rs`: doc-comments.
- Verificado con `grep -rn 'lowfi' src/` que solo quedan referencias a
  `talwat/lowfi` en URLs (correctas — son links al upstream original).

### T4 — Flag `--genre` en `Args` — DONE

En `src/main.rs::Args`:

```rust
#[clap(long, short = 'g')]
genre: Option<String>,
```

Documentado: "The genre preset to play. One of: lofi, synthwave, jazz-lofi,
ambient. Skips the picker if set".

`play::play(cli)` recibe el `Args` con el campo nuevo; el flow de resolución
está en `src/play.rs`.

### T5 — Picker interactivo en `src/player/ui/picker.rs` — DONE

Módulo nuevo `src/player/ui/picker.rs`:

- API: `pub fn pick(data_dir: &Path) -> eyre::Result<Option<String>>`
- Lista de géneros: 4 built-ins (`lofi`, `synthwave`, `jazz-lofi`,
  `ambient`) + cualquier `*.txt` adicional en `data_dir` (excluye
  `micropop.txt` y archivos ocultos).
- TUI con `crossterm`: caja centrada, header "Select a genre", lista
  vertical con highlight bold/dim.
- Controles: `↑`/`k`, `↓`/`j`, `Enter` (confirma), `q`/`Esc` (sale y
  retorna `Ok(None)`).
- Integración en `src/play.rs`: si `args.genre.is_none() && args.tracks.is_none()`,
  llama al picker; si retorna `None` y tampoco hay `--tracks`, sale
  limpio (`Ok(())`).

### T6 — `data/<genre>.txt` para los 4 géneros — DONE

- `data/lofi.txt`: renombrado de `data/lofigirl.txt` (movido, no copiado).
  Lista curada de Lofi Girl con cientos de tracks reales.
- `data/jazz-lofi.txt`, `data/synthwave.txt`, `data/ambient.txt`: nuevos,
  3 tracks cada uno apuntando a Internet Archive. URLs modestas — el
  README aclara que se pueden ampliar/curar editando los archivos.
- `data/micropop.txt`: intacto (custom list de ejemplo del upstream).
- `data/sample.txt`: borrado.

Default actualizado en `src/tracks/list.rs::load`: si no hay `genre` ni
`tracks`, fallback a `"lofi"` (la nueva lista renombrada). El
`include_str!` ahora se selecciona por género en lugar de hardcodear
`lofigirl`.

### T7 — Reescribir `README.md` para `lazylofi` — DONE

- Nombre, tagline, disclaimer dual (Lofi Girl + Internet Archive).
- Installing: misma estructura del upstream con binario `lazylofi`.
- Usage: `lazylofi`, `lazylofi --genre <name>`, `lazylofi --tracks <name>`.
- Controles del player + controles del picker.
- Genre Lists: formato, cómo editar/ampliar.
- Custom Track Lists: path `~/.local/share/lazylofi/`.
- Scraping: subcomando mantenido, documentado como opcional (heredado del
  upstream, útil para curaduría).
- AUR: "planned but not yet published" (honesto).

### T8 — Verificación — DONE

- `cargo check`: 0 errores, 2 warnings (pre-existentes del upstream:
  `List.name` never read, variant `Play` never constructed).
- `cargo clippy --all-features`: 85 warnings restantes, todos pre-existentes.
  Verificado con grep que 0 warnings caen en código nuevo
  (`picker.rs`, `play.rs:78-95`, `player.rs:188`, `List::load` reescrito).
- `cargo build --release`: produce `target/release/lazylofi` (7.8 MB).
- `./target/release/lazylofi --help`: muestra `-g, --genre <GENRE>` con
  descripción correcta.

## Commits / git

**No se hizo commit.** El usuario decide cuándo crear el repo y commitear.
Actualmente no hay `.git/` — el directorio está listo para `git init`
cuando el usuario quiera.

## Out of scope (queda para futuro)

- Empaquetado en AUR, Homebrew, scoop.
- Soporte MPRIS docs (queda como feature opcional, igual que el upstream).
- UI nueva más allá del picker (no se tocó el UI del player).
- Tests automatizados del TUI (el upstream no tiene; no se introduce deuda).
- Ampliar los catálogos de synthwave/jazz lofi/ambient con más tracks reales
  (URLs adicionales a archive.org o Lofi Girl).

## Estado final

- [x] T1 — Limpieza
- [x] T2 — Cargo.toml
- [x] T3 — Rebranding código
- [x] T4 — Flag --genre
- [x] T5 — Picker
- [x] T6 — data/*.txt
- [x] T7 — README
- [x] T8 — Verificación