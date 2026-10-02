# followups-batch-1 — scrape + persist + mostrar género

## Contexto

Tres follow-ups pedidos por el usuario, en orden de menor a mayor impacto:

1. **Arreglar `scrape`**: el subcomando `lazylofi scrape` está hardcodeado
   a `lofigirl.com` (que da 403). Generalizarlo a un URL base configurable o
   sacarlo si no aporta.
2. **Persistir género**: hoy, cada vez que abrís `lazylofi` sin `--genre`,
   arranca con el picker. Queremos que recuerde el último género elegido
   y arranque directo en él (con el picker como fallback si el archivo
   no existe o está vacío).
3. **Mostrar género actual en el UI**: el render del player debería mostrar
   qué género está sonando. Hoy solo muestra el nombre del track.

El #4 (migrar todo el UI a ratatui) queda para su propio feature doc porque
es un refactor grande e independiente.

## Decisiones

### T1 — `scrape`

Hoy: `src/scrape.rs` hace GET a `https://lofigirl.com/wp-content/uploads/`
y scrapea links con `.zip` (default) o la extensión que el usuario pida.
Devuelve una lista de URLs.

Decisión: el usuario dijo "generalizarlo para apuntar a cualquier URL base
o reemplazarlo por algo útil". La opción más útil: hacer que `scrape`
tome un `--base <URL>` flag (default a un valor útil — ej: algún archive
directory). Si el user no pasa `--base`, dar un error claro explicando
cómo usarlo. Mantener el comportamiento scrape-a-STDOUT.

Otra opción más útil: scrapear archive.org por género (búsqueda en
`archive.org/search.php?query=lofi`). Pero eso requiere implementar el
parser del HTML de búsqueda. Más trabajo, mismo outcome.

**Decisión final**: agregar `--base <URL>` flag. Si no se pasa, fallar con
mensaje útil. Mantener el resto de los flags (`--extension`, `--include-full`).

### T2 — Persistir género

Decisión: guardar el último género elegido en el archivo `volume.txt` que
ya existe? No, mejor un archivo separado: `~/.config/lazylofi/genre.txt`
(simple, una línea con el nombre del género).

Flow:
- En `play::play`, después de resolver el género (sea por `--genre`, picker,
  o default), guardar el nombre del género en `~/.config/lazylofi/genre.txt`.
- Si el usuario corre `lazylofi` sin args ni `--genre` ni `--tracks`:
  - Si existe `genre.txt`, leer y usar ese género (sin picker).
  - Si no existe o el archivo está vacío → mostrar el picker como hoy.

**Edge case**: si el género persistido ya no existe (porque el user borró
el `.txt` correspondiente), el picker debe abrirse y mostrar error útil.
Esto se maneja naturalmente: `List::load` devuelve `Err` → el caller cae
al picker.

### T3 — Mostrar género en UI

Decisión: agregar el nombre del género en el render del player. Hoy
`Player` tiene `list: RwLock<List>`, y `List` tiene `name: String`. El UI
puede leer `player.list.read().await.name`. Pero el render corre en un
loop a 12 FPS sin await entre frames... espera, sí usa `sleep().await`,
así que awaitear el read no es problema.

Cambios concretos:
- En `components::action()` (o donde se renderice la línea de "now
  playing"), agregar el nombre del género entre `[` y `]` o como prefijo.
- Ejemplo visual: en vez de `♪ Track Name`, mostrar `♪ [synthwave] Track Name`.
- Si `--tracks` está activo: mostrar `[tracks]` o el nombre del archivo
  (lo que dé más info).

## Tareas

### T1 — Generalizar `scrape`

En `src/scrape.rs`:

- Agregar flag `--base <URL>` (default a `https://archive.org/download/`
  o vacío que fuerce error).
- Si no se pasa `--base`, retornar error claro: "must provide --base".
- Cambiar el `BASE_URL` constante hardcodeada para usar el flag.
- Validar la URL (que tenga `://`).

En `src/main.rs`:

- Agregar el flag `--base` al variant `Commands::Scrape`.

En `README.md`:

- Actualizar la sección "Scraping" con el nuevo uso.

### T2 — Persistir género

En `src/play.rs`:

- Después de resolver el género, llamar a `PersistentGenre::save(&genre)`.
- Si `--tracks` está activo, no persistir (no es un género, es un archivo).

Crear función `PersistentGenre` en `src/play.rs` (o nuevo archivo `src/persist.rs`):

```rust
pub struct PersistentGenre;
impl PersistentGenre {
    async fn path() -> eyre::Result<PathBuf> { /* ~/.config/lazylofi/genre.txt */ }
    async fn load() -> eyre::Result<Option<String>> { /* lee el archivo */ }
    async fn save(genre: &str) -> eyre::Result<()> { /* escribe */ }
}
```

En `src/play.rs::play()`:

- Si el usuario NO pasó `--genre` ni `--tracks`:
  - Intentar `PersistentGenre::load()`.
  - Si retorna `Some(genre)`, usar esa en lugar del picker.
  - Si retorna `None`, mostrar el picker como hoy.
- Después de resolver el género final (por cualquier vía), persistirlo.

### T3 — Mostrar género en UI

En `src/player/ui/components.rs::action()`:

- Agregar el nombre del género antes o después del track name.
- Si `has_custom_tracks`, mostrar `[tracks]` o el nombre del archivo.

**Decisión de layout**: poner el género como prefijo entre brackets, ej:
`synthwave ♪ Track Name` o `[synthwave] Track Name`. Elegir el estilo que
quede mejor con el border actual.

### T4 — Verificación

- `cargo check`: 0 errores.
- `cargo clippy --all-features`: 0 warnings nuevos.
- `cargo build --release`: binario OK.
- `./target/release/lazylofi --help`: `--genre`, `--tracks`, etc. siguen
  presentes.

### T5 — Actualizar README

- Sección "Scraping": nuevo ejemplo con `--base`.
- Sección "Usage": agregar nota sobre persistencia de género.
- Sección "Controls" (no, esa ya está actualizada en feature anterior).

## Out of scope (queda para futuro)

- T4: Migrar UI del player a ratatui (refactor grande, propio feature doc).
- Mostrar historial de géneros en algún lado.
- Persistir volumen por género (cada género con su propio volumen).

## Estado

- [x] T1 — scrape --base flag
- [x] T2 — Persistir género
- [x] T3 — Mostrar género en UI
- [x] T4 — Verificación
- [x] T5 — Actualizar README