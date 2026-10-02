# catalog-expansion — más tracks instrumentales

## Constraint

100% instrumental. Sin voces cantadas, rap ni palabra hablada. La selección se basa en metadatos publicados; no sustituye la comprobación auditiva de cada archivo.

## Resumen

- data/lofi.txt: 8 tracks agregados (era 7, ahora 15).
- data/synthwave.txt: 10 tracks agregados (era 5, ahora 15).
- data/jazz-lofi.txt: 8 tracks agregados (era 7, ahora 15).
- data/ambient.txt: 10 tracks agregados (era 5, ahora 15).

## Colecciones usadas

### Lo-Fi

- **royalty-free-music** — música de fondo de Nver Avetyan, [ficha](https://archive.org/details/royalty-free-music). Evidencia: la ficha incluye la etiqueta «instrumental» y los archivos seleccionados están identificados como chill hop/lo-fi; el índice de descargas enumera sus nombres `.mp3`. La etiqueta corresponde a la colección, no es una certificación independiente de cada pista.
  - Tracks agregados: 8.

### Synthwave

- **royalty-free-music** — colección de música de fondo de Nver Avetyan, [ficha](https://archive.org/details/royalty-free-music). Evidencia: etiqueta «instrumental» de la colección y nombres individuales con «Synthwave», «Retrowave» o «Synthwave House/Cyberpunk» en el listado de MP3.
  - Tracks agregados: 10.

### Jazz Lo-Fi

- **royalty-free-music** — colección de música de fondo de Nver Avetyan, [ficha](https://archive.org/details/royalty-free-music). Evidencia: etiqueta «instrumental» de la colección; títulos individuales «Jazz Chill Hop», «Jazzy Chill Hop» y «Jazzy Calm Background» en el listado de MP3.
  - Tracks agregados: 8.

### Ambient

- **ambientforfilm** — *Ambient Film Music*, de Serge Quadrado, [ficha](https://archive.org/details/ambientforfilm). Evidencia: la descripción declara explícitamente «Slow ambient and positive relaxing meditative instrumental tracks for video»; la lista de descargas identifica los diez nombres `.mp3` incorporados.
  - Tracks agregados: 10.

## Tracks descartados / dudosos

- `lofi-plvgkk`: la ficha indexada enumera MP3, pero no confirma ausencia de voces en cada pista.
- `synthwave-dreams-vol.-1-16`: está etiquetada «Synthpop» y no garantiza material exclusivamente instrumental.
- `dark-synthwave-pyl9yg`: su descripción enumera géneros, pero no confirma ausencia de voces.
- `ambient-sounds-music-for-dreams`: recopilación de distintos artistas sin declaración inequívoca de instrumentalidad para todas las pistas.
- `royalty-free-music/Synthwave Phonk.mp3`: no se incluyó; el subgénero podría incorporar voces o samples hablados.
- `lofi.txt` ya contiene `I_Need_A_Girl_by_l33`: título dudoso, conservado por la instrucción de no eliminar entradas existentes; requiere revisión auditiva.

## Verificación

- `cargo check`: OK; una advertencia preexistente sobre la variante `Play` no utilizada.
- `cargo build --release --all-features`: OK.
- `wc -l data/*.txt`: 15 líneas en cada catálogo objetivo; `micropop.txt` también tiene 15 y no se modificó.
- Comprobación local adicional: 15 URL únicas por género, sin líneas vacías y con prefijo `https://archive.org/download/` y sufijo `.mp3`.

**Límite de verificación:** no se hicieron solicitudes directas a archive.org ni se reprodujeron archivos. Los nombres de MP3 y las descripciones proceden de resultados indexados. No se comprobaron respuestas HTTP individuales ni la ausencia de voces mediante escucha. Se identificó una colección nueva para cada género, no las 2–4 solicitadas; no se inventaron colecciones adicionales.

## Ronda 2 — diversidad de fuentes (lofi)

Tracks agregados: 6

### Colecciones nuevas usadas

- **jamendo-085030** — álbum *Instrumental Hip Hop Delight*, de DJ l'Alien, https://archive.org/details/jamendo-085030. Evidencia instrumental: el título del álbum lo identifica expresamente como instrumental y su descripción asocia `04.mp3` con «Jazzy Beat» y `13.mp3` con «Fluty Business»; ambos archivos respondieron como `audio/mpeg` (HTTP 206).
  - Tracks: 2.
- **jamendo-061248** — álbum *Instrumental Beats Vol.1*, de Shanel, https://archive.org/details/jamendo-061248. Evidencia instrumental: el título del álbum declara «Instrumental Beats» y la descripción identifica `01.mp3` como «Retro Beat» y `05.mp3` como «Birds Inside Us Beat»; ambos archivos respondieron como `audio/mpeg` (HTTP 206).
  - Tracks: 2.
- **jamendo-024161** — álbum *Instrumental Hip-Hop Vol. 1*, de Deff Syndicate, https://archive.org/details/jamendo-024161. Evidencia instrumental: el título del álbum declara «Instrumental Hip-Hop» y la descripción identifica `01.mp3` como «Hip-Hop 1» y `02.mp3` como «Hip-Hop 2»; ambos archivos respondieron como `audio/mpeg` (HTTP 206).
  - Tracks: 2.

### Tracks descartados

- `chillhop-records-chillhop-podcast-001` y `-003`: la búsqueda de identificadores de Archive.org solo devolvió `-002`, ya utilizado.
- `chillhop-essentials-spring-2018` y `Chillhop-Essentials-Fall-2018`: contienen muchos artistas, pero sus descripciones solo indican «Chillhop» y «Lofi», no garantizan instrumentalidad; algunos títulos incluyen colaboraciones («ft.» o «feat.»).
- `AtticBeats004`: pese a anunciar hip-hop instrumental, contiene una mezcla en un único MP3 con títulos asociados a artistas vocales; no se pudo verificar la ausencia de voces en cada segmento.

**Límite de esta ronda:** la clasificación instrumental se apoya en los títulos y listados publicados de los álbumes; se comprobaron los seis enlaces mediante solicitudes parciales HTTP, pero no se escucharon los archivos. La ausencia absoluta de voces requiere revisión auditiva.

## Ronda 2 — diversidad de fuentes (synthwave)

Tracks agregados: 7.

### Colecciones nuevas usadas

- **jamendo-624918** — *Neon Overdrive Protocol*, álbum de DJ CBee SUPREME ([ficha](https://archive.org/details/jamendo-624918)). Evidencia instrumental: la descripción lo define como álbum instrumental de synthwave trap y los cuatro títulos seleccionados incluyen «Instrumental».
  - Tracks: 4.
- **vice-city-nights-the-singles-2020-2022-plv6xx** — *Vice City Nights: The Singles: 2020-2022*, de Lazerbeam Sunset ([ficha](https://archive.org/details/vice-city-nights-the-singles-2020-2022-plv6xx)). Evidencia instrumental: se eligieron exclusivamente las versiones cuyos nombres indican «(Instrumental)», no las versiones originales del mismo álbum.
  - Tracks: 2.
- **komm-s-sser-tod-synthwave-80s-remixes-wm6ln8** — remixes synthwave de Astrophysics ([ficha](https://archive.org/details/komm-s-sser-tod-synthwave-80s-remixes-wm6ln8)). Evidencia instrumental: solo el segundo archivo se denomina «(instrumental)»; los otros dos indican una cantante invitada.
  - Tracks: 1.

### Tracks descartados

- `vice-city-nights-the-singles-2020-2022-plv6xx`: versiones sin la indicación «Instrumental»; el álbum también está etiquetado como synthpop.
- `komm-s-sser-tod-synthwave-80s-remixes-wm6ln8`: versiones con «feat. Marina Rios», por incluir voces.
- `pirate-tapes-the-lost-rainbow-tapes`: pese a que su descripción confirma que es instrumental, predominan la improvisación y el rock experimental, y los MP3 son muy extensos para este catálogo.

**Verificación de esta ronda:** los siete nombres coinciden con archivos MP3 del índice de archive.org; cada URL respondió `206 Partial Content` con `Content-Type: audio/mpeg`. La clasificación instrumental procede de los títulos y las descripciones publicados; no se realizó una escucha completa para descartar voces o muestras habladas inadvertidas.

## Ronda 2 — diversidad de fuentes (jazz-lofi)

Tracks agregados: 5.

### Colecciones nuevas usadas

- **DWK244** — *Back To The Beat Tape*, de Frenic, [ficha](https://archive.org/details/DWK244). La descripción presenta 15 pistas de hip-hop instrumental con samples de jazz, soul y funk; los metadatos de cada MP3 seleccionado indican el género «Instrumental Hip-Hop». No se comprobó por escucha si los samples contienen fragmentos vocales.
  - Tracks: 3 (`3rd Roc`, `Everglade`, `Late Night Bar`).
- **DWK263** — *Dirty Quartet*, de Subjectjazz Records, [ficha](https://archive.org/details/DWK263). La descripción indica jazz, lo-fi e hip-hop instrumental; los metadatos de los dos MP3 seleccionados indican el género «Instrumental Hip-Hop». No se comprobó por escucha la ausencia de voces.
  - Tracks: 2 (`Dirty Quartet Part II`, `Dirty Quartet Part IV`).

### Tracks descartados

- **DWK123**: la descripción menciona samples de chanson y contiene pistas con colaboradores identificados como «feat.»; se descartó por riesgo de voces en los samples.
- **jamendo-639304**: aunque la descripción identifica su única pista como jazz lo-fi instrumental, se priorizaron dos colecciones con varios tracks para mantener la distribución solicitada.

Las cinco URL añadidas se verificaron con solicitudes HEAD: HTTP 200 y `Content-Type: audio/mpeg`. Esta verificación confirma disponibilidad, no el contenido musical; la condición instrumental sigue basada en metadatos y requiere revisión auditiva para una garantía absoluta.

## Ronda 2 — diversidad de fuentes (ambient)

Tracks agregados: 5.

### Colecciones nuevas usadas

- **wh258** — *Parts*, de iaiko, [ficha](https://archive.org/details/wh258). Evidencia de instrumentalidad: el autor describe las cinco piezas como bucles ambientales suaves; la ficha las clasifica como ambient, electrónica y drone, y la API enumera los MP3 de «Part 2» y «Part 5» con artista iaiko y género drone. No menciona voces.
  - Tracks: 2.
- **Ibreathefur-Phosphenes** — *Phosphenes*, de Ibreathefur (Chris Spearman), [ficha](https://archive.org/details/Ibreathefur-Phosphenes). Evidencia de instrumentalidad: la ficha describe las tres piezas como drones ambientales de diseño sonoro abstracto; la API enumera «Waking In Sync» y «A Curvature» como MP3 originales del artista. No menciona voces.
  - Tracks: 2.
- **waag_rel034** — *The Sound of Space*, de Colin Blake, [ficha](https://archive.org/details/waag_rel034). Evidencia de instrumentalidad: la ficha describe paisajes sonoros electrónicos de ambient espacial y acredita la música a Colin Blake; la API identifica «Nocturne (Between Galaxies)» como MP3 original. «Las nubes cantan» es una metáfora de la descripción, no evidencia de voz en la pista seleccionada.
  - Tracks: 1.

### Tracks descartados

- `mt018/Bell Meditation`: la ficha menciona ecos de tradición coral; no se puede descartar voz.
- `SE061-Ishtar-Starseeds/A voice from the stars`: título ambiguo respecto de presencia de voz.
- `CalmPills`: la ficha solo garantiza que las mezclas son *mayormente* instrumentales.

**Verificación de esta ronda:** los cinco archivos figuran como MP3 en las respuestas directas de `archive.org/metadata/<identificador>` y cada URL respondió HTTP 200 con `Content-Type: audio/mpeg` a una solicitud HEAD. La selección instrumental se apoya en las descripciones publicadas; no se realizó escucha, por lo que la ausencia absoluta de voces no está comprobada auditivamente.
