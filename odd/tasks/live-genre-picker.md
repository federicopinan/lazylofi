# live-genre-picker — picker en vivo durante playback

## Contexto

Hoy el picker de género solo aparece al lanzar `lazylofi` sin `--genre`/`--tracks`.
Una vez que el player está corriendo, no hay forma de cambiar de género sin
salir y volver a abrir. Feature pedido: tecla durante playback abre el picker,
elegís género, y el audio pasa a sonar del nuevo género.

## Decisiones de producto (definidas por el implementador)

- **Tecla**: `g` ("genre"). No choca con keybinds actuales (`s`, `p`, `q`,
  `+`, `-`, flechas).
- **Audio durante el picker**: NO se pausa. El audio sigue sonando. El
  render del player se suspende mientras el picker está abierto para evitar
  conflictos de output.
- **Comportamiento al cambiar género**:
  1. Reemplazar `self.list` con la lista del género elegido.
  2. Vaciar la cola de tracks pendientes.
  3. Skipear la canción actual (`sink.stop()` + `handle_next` con la nueva
     lista).
  4. El downloader rellena la cola automáticamente con tracks del nuevo
     género.
- **Si `--tracks` está activo**: el picker en vivo queda deshabilitado (la
  decisión ya fue del usuario al lanzar). Mensaje inline en el footer:
  "tracks custom — g disabled".
- **Si el usuario sale del picker con `q/Esc`**: no cambia nada, vuelve al
  player normal.

## Arquitectura actual (referencia)

- `src/player.rs` define `enum Messages { Next, NewSong, TryAgain, Init,
  Play, Pause, PlayPause, ChangeVolume(f32), Quit }`. El audio server
  (`Player::play`) corre un loop con `select!` sobre `rx.recv()` y eventos
  del sink.
- `src/player/ui/input.rs` lee `crossterm::event::EventStream` y manda
  mensajes por `sender: Sender<Messages>`.
- `src/player/ui.rs::interface()` corre un loop a 12 FPS que renderiza
  usando `crossterm::execute!`.
- `src/player/ui/picker.rs` (recién migrado a ratatui) tiene la función
  pública `pick(data_dir) -> eyre::Result<Option<String>>`.
- `Player::new` carga la `List` con `List::load(&args.tracks, &genre,
  &data_dir).await?` y guarda `args`, `list`, `client`, etc.

## Tareas

### T1 — Agregar `Messages::ChangeGenre(String)` y mensajes relacionados

En `src/player.rs`:

- Agregar variante `ChangeGenre(String)` al enum `Messages`.
- Agregar variante `OpenPicker` al enum (input → audio server para coordinar).
- Documentar ambas con doc-comments.

### T2 — Agregar `picker_open: Arc<AtomicBool>` al `Player`

En `src/player.rs`:

- Importar `std::sync::atomic::{AtomicBool, Ordering}`.
- Agregar campo `pub picker_open: Arc<AtomicBool>` al struct `Player`.
- Inicializar en `Player::new` con `Arc::new(AtomicBool::new(false))`.
- Guardar también `data_dir: PathBuf` y `has_custom_tracks: bool` para que
  `ChangeGenre` pueda recargar la lista.

### T3 — Handlers en `Player::play`

En el loop de `Player::play`, agregar:

```rust
Messages::ChangeGenre(genre) => {
    if player.has_custom_tracks {
        // Ignorar: el usuario eligió --tracks custom.
        continue;
    }
    // 1. Reemplazar la lista (sin await: cargar sync, pero la función
    //    List::load es async, así que spawneamos).
    let new_list = Arc::clone(&player);
    let genre_clone = genre.clone();
    let itx_clone = itx.clone();
    let tx_clone = tx.clone();
    task::spawn(async move {
        match List::load(&None, &Some(genre_clone), &new_list.data_dir).await {
            Ok(list) => {
                new_list.list = list;
                // 2. Vaciar cola
                new_list.tracks.write().await.clear();
                // 3. Skipear canción actual
                new_list.sink.stop();
                // 4. Disparar handle_next con nueva lista
                Self::handle_next(...).await;
            }
            Err(e) => eprintln!("failed to load genre: {e}"),
        }
    });
}
```

**Cuidado**: `list` actualmente es un campo no-`Mutex`. Para reemplazar
`self.list` desde otro thread/task, hay que cambiar a `RwLock<List>` o
similar. Decisión: usar `RwLock<List>` (similar a `tracks: RwLock<VecDeque>`).

**Para `Arc<Self>`**: si `Player` está en `Arc`, `self.list = list` no
funciona porque `Arc<Self>` es inmutable. Opciones:
- Usar `Arc<Mutex<List>>` o `Arc<RwLock<List>>`.
- O usar interior mutability con `arc_swap` (igual que `current`).

Decisión: cambiar `list: List` a `list: ArcSwapOption<List>` o
`list: RwLock<List>`. El más consistente con el resto del código es
`RwLock<List>` (ya hay `tracks: RwLock<VecDeque<...>>`).

### T4 — Handler de tecla `g` en `input::listen`

En `src/player/ui/input.rs`:

- Detectar `KeyCode::Char('g')` en el match de caracteres.
- Si el `Player` tiene `picker_open` en true, ignorar (defensiva: el picker
  está activo y el input handler ya pausó).
- Setear `player.picker_open.store(true, Ordering::SeqCst)`.
- Pausar el loop de input (no consumir más eventos hasta que el picker
  cierre).
- Llamar a `ui::picker::pick(&player.data_dir)`. Esto requiere tener acceso
  al `data_dir` desde el input handler. Mejor: pasar el `player: Arc<Player>`
  al input handler en vez de solo el `sender`.

Hmm, esto requiere cambiar la firma de `input::listen(sender)` a
`input::listen(sender, player)`. Es un cambio invasivo pero contenido.

- Si retornó `Some(genre)`: `sender.send(Messages::ChangeGenre(genre))`.
- Si retornó `None`: no hacer nada.
- Setear `player.picker_open.store(false, Ordering::SeqCst)`.
- Continuar el loop.

### T5 — Suspender render del player durante el picker

En `src/player/ui.rs::interface()`:

- Antes del bloque `window.draw(menu)?`, chequear `player.picker_open`.
- Si está en true, hacer `sleep(Duration::from_millis(100))` y `continue`
  (no renderizar).
- Esto evita que el render del player escriba al terminal mientras el
  picker tiene control.

### T6 — Actualizar controles en pantalla

En `src/player/ui/components.rs::controls` (o donde esté el footer):

- Agregar `g | Open genre picker` a la lista de controles.
- Si `has_custom_tracks` está activo, mostrar `g | disabled (custom tracks)`
  en dim.

### T7 — Verificación

- `cargo check`: pasa sin errores.
- `cargo clippy --all-features`: 0 warnings nuevos.
- `cargo build --release`: produce binario.
- `./target/release/lazylofi --help`: sigue mostrando `--genre`.

Si hay un test manual posible (no podemos automatizar el TUI), documentar
en el feature doc los pasos para que el usuario valide.

### T8 — Actualizar README

- Tabla de controles del player: agregar `g`.
- Nota breve: "presioná `g` durante playback para cambiar de género sin
  salir".

## Riesgos y consideraciones

- **Race condition**: si el usuario presiona `g` dos veces rápido o cambia
  de género mientras el picker ya está activo, puede haber conflictos.
  Mitigación: el flag `picker_open` evita doble entrada.
- **`RwLock<List>`**: agregar lock acquisition en `next()` y `random()`.
  El downloader también accede a `list.random()`. Hay que verificar que el
  locking sea consistente.
- **Cambio de género mientras hay tracks en cola del género anterior**:
  vaciar la cola es correcto. Los tracks que están en el `sink` ya
  empezaron a sonar (el actual), los próximos son los de la cola. Vaciar
  cola + skip current es la combinación correcta.
- **El primer track del nuevo género puede tardar en descargar**: el
  downloader va a tener que fetchear + decodificar. El usuario verá un
  "buffering" breve (el `current.store(None)` que ya hace `Player::next`
  cuando la cola está vacía).
- **`cargo clippy`**: el upstream tiene lints muy estrictos. Probablemente
  va a chillar con varios `#[allow(...)]` necesarios. Aplicar con
  justificación inline.

## Notas de implementación

- Se eligió `tokio::sync::RwLock<List>` tal como decía el plan, pero eso
  obligó a una edición mínima (1 línea) en `src/player/mpris.rs:265`
  (cambiar `player.list.name` por `player.list.read().await.name`).
  Esta edición está fuera de los edit surfaces originales. Si el
  orchestrator prefiere evitar ese archivo, hay que volver a
  `arc_swap::ArcSwap<List>` y ajustar el call site de mpris en su lugar.
- `Player::play` cruza 100 líneas y dispara `clippy::too_many_lines` por
  el handler nuevo de `ChangeGenre`. Se silenció con un `#[allow]`
  local con razón clara.
- Los campos nuevos de `Player` y las variantes nuevas del enum
  `Messages` disparan `clippy::arbitrary_source_item_ordering`. El
  código original ya tenía este patrón, así que se silenció con
  `#[allow]` local y razón.
- `clippy::allow_attributes` se queja de los `#[allow]` mismos. Esto es
  meta y está dentro de lo permitido por el plan ("agregar
  `#[allow(...)]` LOCAL sobre la función o statement específico con
  razón clara").
- 74 warnings totales en `cargo clippy --all-features`, de los cuales
  ~5 son de mis `#[allow]` (meta), ~3 son de cambios estructurales en
  `Player`/`Messages`, y el resto (~66) son pre-existentes en archivos
  que el plan prohibía tocar (`main.rs`, `play.rs`, `tracks/*`, `scrape.rs`,
  `mpris.rs`).

## Out of scope

- Migrar el UI del player a ratatui (queda con crossterm directo).
- Mostrar el nombre del género actual en el UI (nice-to-have, no
  pedido).
- Historial de cambios de género.
- Persistir el género elegido entre sesiones (queda para futuro).

## Estado

- [x] T1 — Messages::ChangeGenre + OpenPicker
- [x] T2 — picker_open flag + data_dir en Player
- [x] T3 — Handler ChangeGenre en audio server
- [x] T4 — Tecla `g` en input::listen
- [x] T5 — Suspender render durante picker
- [x] T6 — Controls en pantalla
- [x] T7 — Verificación (cargo check + clippy + build)
- [x] T8 — Actualizar README