# fix-tracks-and-ratatui — bug fix de URLs + refactor del picker

## Contexto

El usuario reportó "no funciona" después del fork inicial. Diagnóstico:

1. **`data/synthwave.txt`, `data/jazz-lofi.txt`, `data/ambient.txt`** — los
   identifiers que había puesto el subagent (`SynthwaveComp`,
   `KRLX-Sep-2018-Mix`, `AmbientDrone`) son inventados. archive.org devuelve
   503 ("Internet Archive: Error") con HTML.
2. **`data/lofi.txt`** — la lista venía del upstream `lowfi` y apunta a
   `lofigirl.com`, que devuelve 403. El sitio cambió su política y bloquea
   descargas directas sin headers específicos. **Esto estaba roto en el
   upstream original**, no lo introdujimos nosotros.

Resultado: **las 4 listas no funcionan**, la app no descarga nada.

Adicionalmente, el usuario pidió mejorar la TUI: navegar entre géneros
mientras suena, mejor feedback visual, etc. La opción elegida es migrar
**solo el picker** a ratatui (alcance limitado, no tocar el player).

## Decisiones

- **Abandonar Lofi Girl** como fuente. Está roto a nivel de protocolo del
  sitio. Pasamos a usar Internet Archive también para `lofi`.
- **URLs absolutas** en los `.txt`, no base+path. Evita problemas de
  concatenación con caracteres especiales (em dash, full-width chars,
  espacios en subcarpetas). Cada línea es una URL completa.
- **Todas las MP3**, no FLAC. `rodio` solo tiene `symphonia-mp3` activado
  en Cargo.toml.
- **Solo el picker se migra a ratatui**. El UI del player queda con
  crossterm directo (alcance limitado, menor riesgo).

## Tareas

### T1 — Reemplazar `data/lofi.txt` con URLs reales — DONE

7 tracks validados con HTTP 200 (mezcla chillhop + Jamendo + Jazz One):

- chillhop-records-chillhop-podcast-002 / Akryl - Mocha.mp3
- chillhop-records-chillhop-podcast-002 / Alicks - Pack your things.mp3
- jamendo-506817 / 01-1995378-The_Mountain-Chill LoFi Hip-Hop.mp3
- jamendo-632023 / 01-2315669-Databend-Lo Fi Hip Hop Downtempo Electronic.mp3
- soundcloud-615981939 / No_Copyright_Chill_Lofi_Hiphop_-_I_Need_A_Girl_by_l33-615981939.mp3
- DWK243 / Jazz_One_-_05_-_Feather.mp3
- DWK284 / Jazz_One_-_01_-_As_Time_Goes_By.mp3

### T2 — Reemplazar `data/synthwave.txt` con URLs reales — DONE

5 tracks:

- synthwave-artifacts-retro-wave-2020 / VA – Synthwave Artifacts (2020) / 001. Khrigar - Tranquility In The Abyss.mp3
- synthwave-artifacts-retro-wave-2020 / VA – Synthwave Artifacts (2020) / 005. N3v1773 - Unreleased.mp3
- the-gates-synthspace-anthology / The Gates Synthspace Anthology / 001. Stars Crusaders - Gemini (Decoded Feedback Remix).mp3
- the-gates-synthspace-anthology / The Gates Synthspace Anthology / 005. T.O.Y. - The Darkness & The Light (Solitary Experiments Remix).mp3
- jamendo-552500 / 01-2139446-Oleg Silukov-Synthwave Deep Ambient.mp3

### T3 — Reemplazar `data/jazz-lofi.txt` con URLs reales — DONE

7 tracks (Jazz One tiene material muy bueno para esto):

- DWK243 / Jazz_One_-_03_-_Barefoot_Ballet.mp3
- DWK243 / Jazz_One_-_05_-_Feather.mp3
- DWK284 / Jazz_One_-_01_-_As_Time_Goes_By.mp3
- DWK284 / Jazz_One_-_04_-_In_A_Sentimental_Mood.mp3
- jamendo-579069 / 01-2201540-An Ki-Chill Lofi Jazz Hip Hop.mp3
- lo-fi-smooth-jazz-sweet-bossa-nova-for-focus / LoFi Smooth Jazz & Sweet Bossa Nova for Focus.mp3
- the-crisp-cool-air-autumn-lo-fi-jazzhop / The Crisp Cool Air Autumn LoFi Jazzhop Mix.mp3

### T4 — Reemplazar `data/ambient.txt` con URLs reales — DONE

5 tracks:

- swt-ambientcollection / 01_062907.mp3
- swt-ambientcollection / 02_And_Love_Must_Die.mp3
- swt-ambientcollection / 05_Galaxy_1.mp3
- ambient-time-py7kvd / OSC - Ambient Time - 01 Grass Lands.mp3
- ambient-time-py7kvd / OSC - Ambient Time - 03 Neddy's Sanctum.mp3

### T5 — Recompilar y verificar — DONE

- `cargo check`: pasa sin errores.
- `cargo build --release`: produce `target/release/lazylofi` (7.8 MB).

Validación externa de URLs: curl con `-sL -o /dev/null -w "%{http_code}"`
devolvió 200 para todas las URLs en las 4 listas (20/20 OK).

### T6 — Refactor del picker a ratatui — DONE

Migración de `src/player/ui/picker.rs` a ratatui 0.30. Resultado:

- 160 líneas (antes ~200 con crossterm directo).
- API pública intacta: `pub fn pick(data_dir: &Path) -> eyre::Result<Option<String>>`.
- Usa `ratatui::init()` / `ratatui::restore()` (manejo automático de raw mode).
- Widgets: `List` + `ListState` + `Block::bordered().title(...)`.
- Layout: `Layout::vertical/horizontal` con `Flex::Center` para centrado.
- Highlight: `▶` marker + `Color::Cyan` bold.
- Event loop con `event::poll(Duration::from_millis(100))` para no quemar CPU.
- 0 warnings nuevos en `picker.rs`. 2 warnings pre-existentes (no introducidos).
- `Cargo.toml`: `ratatui = { version = "0.30", default-features = false, features = ["crossterm_0_28"] }`.

### T7 — Actualizar README — DONE

- Disclaimer: solo "Internet Archive" como fuente (explicación inline de
  por qué no Lofi Girl).
- Sección "Genre Lists": reorganizada con fuentes por género y notas sobre
  cómo ampliar.
- Sección "Scraping": ajustada para reflejar que ya no es de Lofi Girl.
- Sección "Custom Track Lists": ejemplo actualizado para usar archive.org.

## Out of scope (queda como follow-up)

- **Picker "en vivo" durante playback**: cambiar de género sin salir de
  la app. Necesita tocar el loop del player (no solo el UI). Decisión del
  usuario.
- **Keybinds visibles en pantalla**: el picker actual no muestra los hints
  de `↑/↓/Enter/q`. Se puede agregar un footer con `Block`. Decisión del
  usuario.
- **`src/scrape.rs`**: hardcodeado a `lofigirl.com`. No se tocó porque está
  fuera de scope. Si querés que el scrape vuelva a ser útil, hay que
  cambiar el `BASE_URL` o generalizarlo.
- **Más tracks en las listas**: 5-7 por género es suficiente para probar.
  Si querés más rotación, editá los `.txt`.

## Estado final

- [x] T1 — data/lofi.txt
- [x] T2 — data/synthwave.txt
- [x] T3 — data/jazz-lofi.txt
- [x] T4 — data/ambient.txt
- [x] T5 — Recompilar
- [x] T6 — Refactor picker a ratatui
- [x] T7 — Actualizar README