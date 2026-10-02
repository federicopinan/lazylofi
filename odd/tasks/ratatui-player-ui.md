# ratatui-player-ui — migrar UI del player a ratatui

## Contexto

El UI del player hoy usa crossterm directo (`src/player/ui.rs`,
`src/player/ui/components.rs`). El picker ya está migrado a ratatui
(`src/player/ui/picker.rs`) pero toma control fullscreen con
`ratatui::init()/restore()` y bloquea el input thread con
`crossterm::event::read()`.

Objetivo: migrar todo el UI del player a ratatui, unificar el manejo del
terminal en un solo `DefaultTerminal`, y convertir el picker en un widget
modal overlay que se renderiza encima del player (resolviendo el bloqueo
del input thread y permitiendo ver el player de fondo mientras se elige
género).

## Arquitectura actual (referencia)

- `ui.rs::Window::draw(content: Vec<String>)` — renderiza con `crossterm::execute!` un marco con `┌─┐│ └─┘` y las líneas.
- `ui.rs::interface()` — loop async a 12 FPS que arma 3 strings (`action`, `middle`, `controls`) y las pasa a `Window::draw`.
- `ui.rs::Environment` — maneja alternate screen, raw mode, keyboard enhancement flags.
- `components.rs` — funciones sync que devuelven `String` formateadas con `crossterm::style::Stylize` para bold/dim/color.
- `input.rs::listen` — `EventStream` async de crossterm, match de keys. Caso `g` → invoca `picker::pick()` directo (bloquea el input thread con `event::read()`).
- `picker.rs::pick` — `ratatui::init()` + loop con `terminal.draw` + `event::poll(100ms)` + `event::read()`. Retorna `Option<String>`.
- `Player::play` recibe `Messages` por un canal y maneja el audio server.

## Arquitectura objetivo

```text
┌─────────────────────────────────────────────────┐
│               tokio runtime                     │
│                                                 │
│   task A: ratatui::init() → loop {              │
│     terminal.draw(|frame| {                     │
│       render_player(frame, &app)                │
│       if app.show_picker {                      │
│         render_picker_modal(frame, &app)        │
│       }                                         │
│     })                                          │
│   } → ratatui::restore()                        │
│                                                 │
│   task B: input handler — actualiza App state,  │
│     manda Messages al audio server.             │
│                                                 │
│   task C: Player::play — audio server.          │
│                                                 │
└─────────────────────────────────────────────────┘
```

- **Un solo `DefaultTerminal`** para toda la app. No más `Environment`,
  no más `ratatui::init()/restore()` repetidos.
- **`App` struct** que contiene todo el estado mutable de la UI:
  - `volume_timer: usize` (reemplaza el `VOLUME_TIMER` global).
  - `show_picker: bool`.
  - `picker_state: ListState` (del picker existente).
  - `picker_genres: Vec<String>` (géneros disponibles para el picker en vivo).
  - Una referencia al `Player` (para leer current track, sink volume, etc.).
- **Loop principal async** que hace `terminal.draw(...)` a 12 FPS. Dentro
  del draw: render del player normal + (si `show_picker`) render del
  picker como widget centrado encima.
- **Input handler** lee eventos de crossterm y actualiza el `App`. Cuando
  se necesita afectar al audio, manda `Messages` al audio server.

## Decisiones de producto

- **Picker modal overlay**: el picker se renderiza como un `List` widget
  centrado encima del render del player. El player sigue visible de fondo
  (con un dimming leve para dar foco al modal). Esto reemplaza el
  fullscreen.
- **Sin `Environment`**: se elimina. `ratatui::init()` se llama UNA vez al
  arrancar la UI; `ratatui::restore()` al cerrar.
- **Sin `VOLUME_TIMER` global**: pasa a ser un campo del `App`.
- **Sin `picker_open: Arc<AtomicBool>`**: `App.show_picker: bool` lo
  reemplaza. El input handler checa `app.show_picker` para decidir si
  procesa keys del picker o del player.
- **`--alternate` y `--minimalist`**: se mantienen como antes.
- **Compatibilidad con flags**: `--width` y `--minimalist` se siguen
  aceptando con la misma semántica.

## Tareas

### T1 — Crear `App` struct en `src/player/ui/app.rs`

```rust
pub struct App {
    pub player: Arc<Player>,
    pub show_picker: bool,
    pub picker_genres: Vec<String>,
    pub picker_state: ListState,
    pub volume_timer: usize,
    pub last_volume_change: Option<Instant>,
}

impl App {
    pub fn new(player: Arc<Player>, picker_genres: Vec<String>) -> Self {
        Self {
            player,
            show_picker: false,
            picker_genres,
            picker_state: ListState::default().with_selected(Some(0)),
            volume_timer: 0,
            last_volume_change: None,
        }
    }

    /// Decrements the volume timer; called every frame from the render loop.
    pub fn tick_volume_timer(&mut self) {
        if self.volume_timer > 0 {
            self.volume_timer = self.volume_timer.saturating_sub(1);
        }
    }

    /// Bumps the volume timer when the user changes volume.
    pub fn bump_volume_timer(&mut self) {
        self.volume_timer = AUDIO_BAR_DURATION_FRAMES;
    }
}
```

### T2 — Refactor `components.rs` a widgets ratatui

Las funciones sync que devolvían `String` ahora devuelven widgets:

```rust
```rust
// Antes: fn action(player, current, width) -> String
// Ahora: fn action(player, current, area) -> impl Widget

pub fn action(player: &Player, current: Option<&Arc<Info>>, area: Rect) -> Paragraph<'static>
pub fn progress_bar(player: &Player, current: Option<&Arc<Info>>, area: Rect) -> Gauge<'static>
pub fn audio_bar(volume: f32, percentage: &str, area: Rect) -> Gauge<'static>
```rust
pub fn controls(player: &Player, area: Rect) -> Paragraph<'static>
```

Migrar `genre_prefix()` a un Span o un Paragraph chico.

### T3 — Refactor `ui.rs::interface` y `ui.rs::start`

```rust
async fn render_loop(
    terminal: &mut DefaultTerminal,
    app: Arc<RwLock<App>>,
) -> eyre::Result<()> {
    loop {
        let mut app_guard = app.write().await;
        app_guard.tick_volume_timer();
        terminal.draw(|frame| {
            render_frame(frame, &app_guard);
        })?;
        drop(app_guard);
        sleep(FRAME_DELTA).await;
    }
}

fn render_frame(frame: &mut Frame, app: &App) {
    // Layout principal: bordered block con 3 líneas adentro
    let outer = Block::bordered().title(...);
    let inner = outer.inner(frame.area());
    let chunks = Layout::vertical([
        Constraint::Length(1),  // action
        Constraint::Length(1),  // progress / audio
        Constraint::Length(1),  // controls (si !minimalist)
    ]).split(inner);

    // Render cada chunk con su widget
    frame.render_widget(action_widget, chunks[0]);
    // ...

    if app.show_picker {
        render_picker_modal(frame, app);
    }
}
```

```rust
pub async fn start(player: Arc<Player>, sender: Sender<Messages>, args: Args) -> eyre::Result<()> {
    // Cargar géneros para el picker.
    let picker_genres = collect_genres(&player.data_dir).unwrap_or_default();

    // Crear App.
    let app = Arc::new(RwLock::new(App::new(Arc::clone(&player), picker_genres)));

    // Iniciar ratatui.
    let mut terminal = ratatui::init();

    // Render loop.
    let render = task::spawn(render_loop(&mut terminal, Arc::clone(&app)));

    // Input loop (bloquea hasta Quit).
    input::listen(sender, Arc::clone(&app)).await?;

    render.abort();
    ratatui::restore();

    Ok(())
}
```

**Cuidado**: `ratatui::init()` y `ratatui::restore()` SOLO se llaman aquí, no
en `picker.rs`. El picker pasa a ser un widget que se renderiza dentro del
mismo `Frame`.

### T4 — Refactor `picker.rs`

El picker deja de tener su propio `DefaultTerminal` y `event::read()`.
Pasa a ser **solo render + state update**:

```rust
/// Renders the picker as a centered modal overlay. Does NOT handle input;
/// the input handler is responsible for updating `app.picker_state` and
/// `app.show_picker`.
pub fn render_picker_modal(frame: &mut Frame, app: &App) {
    // Clear behind for focus.
    frame.render_widget(Clear, /* area of modal */);
    let list = List::new(items)
        .block(Block::bordered().title(" Select a genre ".bold()))
        .highlight_style(...)
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, area, &mut app.picker_state.clone());
}
```

**Eliminar** `pick()` y `run_picker()`. El picker ya no se invoca como
función que bloquea. El control pasa al input handler.

### T5 — Refactor `input.rs`

El input handler ahora actualiza el `App` y manda mensajes al audio server.

```rust
pub async fn listen(
    sender: Sender<Messages>,
    app: Arc<RwLock<App>>,
) -> eyre::Result<()> {
    let mut reader = EventStream::new();
    loop {
        let Some(Ok(event::Event::Key(event))) = reader.next().fuse().await else { continue };
        if event.kind == KeyEventKind::Release { continue; }

        let mut app_guard = app.write().await;

        // Si el picker está abierto, sus keys tienen prioridad.
        if app_guard.show_picker {
            match event.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    let i = app_guard.picker_state.selected().unwrap_or(0);
                    app_guard.picker_state.select(Some(i.saturating_sub(1)));
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let i = app_guard.picker_state.selected().unwrap_or(0);
                    if i + 1 < app_guard.picker_genres.len() {
                        app_guard.picker_state.select(Some(i + 1));
                    }
                }
                KeyCode::Enter => {
                    let i = app_guard.picker_state.selected().unwrap_or(0);
                    if let Some(genre) = app_guard.picker_genres.get(i).cloned() {
                        let _ = sender.send(Messages::ChangeGenre(genre)).await;
                    }
                    app_guard.show_picker = false;
                }
                KeyCode::Char('q') | KeyCode::Esc => {
                    app_guard.show_picker = false;
                }
                _ => {}
            }
            drop(app_guard);
            continue;
        }

        // Keys del player normal.
        let msg = match event.code {
            // ... existing matches ...
            KeyCode::Char('g') => {
                if !app_guard.player.has_custom_tracks {
                    app_guard.show_picker = true;
                    app_guard.picker_state.select(Some(0));
                }
                None  // No message to send.
            }
            // ... volume keys: bump_volume_timer() ...
            _ => continue,
        };

        if let Some(msg) = msg {
            if matches!(msg, Messages::ChangeVolume(_)) {
                app_guard.bump_volume_timer();
            }
            let _ = sender.send(msg).await;
        }
        drop(app_guard);
    }
}
```

### T6 — Verificación

- `cargo check`: 0 errores.
- `cargo clippy --all-features`: 0 warnings nuevos.
- `cargo build --release`: binario OK.

Para los lints: el upstream tiene `clippy::restriction` muy estricto. Si
ratatui genera nuevos warnings, agregar `#[allow(...)]` locales con razón
explícita. NO agregar allows globales.

### T7 — Smoke test (no automatizable, paso a user)

- `./target/release/lazylofi --genre lofi` arranca y renderiza.
- Presionar `g` abre el picker como overlay (no fullscreen, se ve el player
  de fondo).
- Elegir otro género cierra el picker y cambia el audio.
- `q` cierra el picker sin cambiar nada.
- `+/-` cambia el volumen y muestra la barra de audio por unos frames.
- `s` skip, `p` pause.

## Archivos a tocar

- `src/player/ui.rs` — reescribir `interface` y `start`. Eliminar
  `Window` y `Environment`.
- `src/player/ui/components.rs` — reescribir para devolver widgets ratatui.
- `src/player/ui/input.rs` — actualizar para usar `App` (no más invocación
  directa de `picker::pick`).
- `src/player/ui/picker.rs` — reescribir para ser solo render de un modal.
  Eliminar `pick()` y `run_picker()`.
- `src/player/ui/app.rs` (nuevo) — `App` struct.
- `src/player/ui/mod.rs` (si existe, ajustar `pub mod`).
- `src/player.rs` — eliminar `picker_open: Arc<AtomicBool>` (ya no hace
  falta; lo maneja `App.show_picker`).

## Out of scope

- Cambios visibles al usuario (los keybinds siguen iguales).
- Migrar el audio server (sigue con `rodio` directo).
- Tests automatizados del TUI.

## Estado

- [x] T1 — App struct
- [x] T2 — components a widgets ratatui
- [x] T3 — ui.rs::start con ratatui::init()/restore()
- [x] T4 — picker.rs como render modal
- [x] T5 — input.rs usa App
- [x] T6 — Verificación
- [ ] T7 — Smoke test (manual, queda al usuario)

### Notas de implementación

- `picker::pick()` y `picker::run_picker()` **se preservaron** porque
  `src/play.rs` los usa para el selector inicial pre-resume (no estaba
  en el alcance de edición). `play.rs` no se podía tocar según las
  edit surfaces. El nuevo `render_picker_modal` convive con ellos.
- `Messages::OpenPicker` se eliminó: el input handler ahora abre el
  picker directamente mutando `app.show_picker`. La variante del enum
  y el match arm correspondiente se borraron de `player.rs`.
- `App::bump_volume_timer` y `App::tick_volume_timer` se declararon
  `const fn` (cumple con el lint `clippy::missing_const_for_fn`).
- `App::last_volume_change` del plan original se omitió: era dead code
  sin consumidor real. El struct final tiene solo los campos
  efectivamente usados.
- `cargo check`: 0 errores. `cargo clippy --all-features`: 0 warnings
  nuevos en archivos modificados (los 61 warnings restantes son
  pre-existentes en `main.rs`, `play.rs`, `player.rs`, `mpris.rs`,
  `downloader.rs`, `tracks.rs`, `tracks/list.rs`). `cargo build
  --release --all-features`: OK.