# Fase T — TUI: cockpit en terminal sobre `a320_sim`

Nota de diseño de la fase T (track paralelo de frontend; decisiones [D-018](decisiones.md)
y [D-019](decisiones.md)). Estado: **cockpit completo derivado del modelo YAML** en
`feat/tui-poc` (2ª iteración sobre la PoC). La fase completa (issues, captura en el
README) queda pendiente de abrir su EPIC.

## Qué es

Un tercer frontend sobre la misma Control/Observe API: el REPL es la ventana
experta, el MCP la del LLM, y la TUI la ventana *visual* — ver los sistemas y
paneles del avión dentro de la terminal, en vivo. Sustituye conceptualmente al
`watch` del REPL (ANSI a pelo, lista plana) por una app Textual con paneles.

La pantalla es una **cuadrícula 2×2 de cuadrantes con scroll independiente**
(el cockpit real apilado son ~95 filas y no cabe en ninguna terminal; F1–F4
enfocan cuadrante):

```
┌ StatusBar: t, ▶/⏸, xN, failures ──────────────────────────┐
├──────────────────────────┬────────────────────────────────┤
│ OVERHEAD (aft+fwd) [F1]  │ GLARESHIELD · MAIN PANEL [F2]  │
│  3 columnas fieles al    │  FCU · EFIS ×2 · warnings      │
│  mockup, ELEC cableado   │  PFD/ND · ISIS · gear/brakes   │
├──────────────────────────┼────────────────────────────────┤
│ PEDESTAL [F3]            │ ELEC SD · E/WD [F4]            │
│  MCDU/RMP/ACP ×2, ECAM   │  synoptic + ECAM + SCENARIO    │
│  CP, thrust, flaps...    │  (world: GPU)                  │
├──────────────────────────┴────────────────────────────────┤
│ command line (gramática del REPL) + log                   │
└───────────────────────────────────────────────────────────┘
```

## Arquitectura (paquete `tui/`, ver árbol en `tui/README.md`)

- **`SimBridge`** (`a320_tui/sim_bridge.py`): único dueño del `a320_sim.Sim`.
  `Sim` es *unsendable*, así que el bridge asserta el thread-id en cada método
  y todo el tick corre en el event loop principal (`set_interval` de Textual,
  ~5 Hz, dt=200 ms — el patrón del `watch`). Prohibido `@work(thread=True)`
  para cualquier cosa que toque el bridge.
- **`SimState`** (`state.py`): dataclass frozen con `t`, las vars del manifest
  y los failures activos. Los widgets renderizan solo desde `SimState` (render
  puro, testeable sin terminal) y nunca tocan el `Sim`; actúan emitiendo
  `KorryButton.Pressed`, que la app traduce a `bridge.set(...)`.
- **`model/`** (D-019): el YAML vendorizado (`a320-controls-model.yaml`,
  copia byte-idéntica de la spec externa) + `loader.py`: 423 `ControlDef`
  tras instanciación ×2/×3 (la tabla vive en el parser; el YAML solo la marca
  en comentarios) y expansión de LSK/SYS_PAGES. Ids canónicos únicos
  (`EFIS_CAPT.FD`, `RMP_3.VHF1`).
- **`cockpit_state.py` + `controller.py`**: todo control del modelo es
  interactuable aunque no tenga sim detrás — korrys latchean, guardas en dos
  pasos, fire pb saltan, selectores/palancas con topes, knobs con clamp,
  push/pull del FCU — sobre estado local puro (`CockpitRegistry`), renderizado
  vía `ControlView` frozen. El `CockpitController` enruta: id cableado →
  `bridge.set`; el resto → registro local.
- **`wiring.py`**: el binding declarativo YAML↔sim por id canónico (catálogo
  ELEC + extras por LVAR crudo + APU). `manifest.py` conserva el lado de
  observación: `manifest_vars()` es el `get` selectivo del tick (~30 vars) —
  **nunca** `snapshot()`. El tick refresca **solo** widgets cableados +
  synoptic + E/WD; los ~410 locales solo se repintan al actuarlos.
- **`layouts/`**: la geometría como datos por zona, transcrita de los mockups
  de `tui/docs/` (`AutoSection` por defecto — coloca una sección YAML entera —
  y filas a mano donde el mockup fija geometría, como el 35VU). El test de
  cobertura garantiza que las cuatro zonas colocan los 423 controles
  exactamente una vez.
- **`EmbeddedRepl`** (`commands.py`): subclase de `A320Repl` con stdout en un
  buffer, ejecutada con `onecmd()` por línea — una sola gramática. Overrides:
  `watch` (la TUI ya es un watch), `run` capado a 30 s (bloquearía el event
  loop), `quit` cierra la app. Los comandos `fail`/`unfail`/`failures` viven
  aquí temporalmente hasta que feat/14 los traiga al REPL (entonces se borran).
- **Modos de tiempo**: `space` pausa, `+`/`-` multiplicador x1–x32 (mismo
  coste de render, más `dt` por tick).

## Geometría 35VU (el panel real)

El overhead replica la disposición del panel ELEC del A320 (35VU), transcrita
del cockpit del A32NX (imagen de referencia `ELEC-Panel.jpg` de
docs.flybywiresim.com — usada como **referencia al maquetar, no como asset**:
el repo es GPLv3 y una foto no sería manipulable en terminal):

```
COMMERCIAL   [27.7V] BAT 1  BAT 2 [27.9V]   AC ESS FEED
GALY & CAB    DC BUS 1 ↑ AC ESS BUS ↑ DC BUS 2          <- mímico pintado
IDG 1   GEN 1   APU GEN   BUS TIE   EXT PWR   GEN 2   IDG 2
```

La geometría vive como **datos** en `layouts/overhead.py` (sección ELEC del
cuadrante OVERHEAD, filas transcritas a mano). Resolución de slots en
`widgets/zone_panel.py`:

- id en `WIRING` — `KorryButton` cableado (catálogo curado o LVAR crudo,
  D-008/D-009; los extras AC ESS FEED / COMMERCIAL / GALY & CAB verificados
  contra el vendor `a320_systems/src/electrical/mod.rs:283-286` y contra el
  registro vivo por `test_wiring.py`).
- `BAT_DISPLAY` — los voltímetros reales de batería en un readout vivo
  (`ELEC_BAT_{1,2}_POTENTIAL`).
- `mimic:` — el mímico de buses va **pintado** (verde estático), como en el
  avión: las líneas del 35VU real no se encienden con el estado — la imagen
  viva es del synoptic.
- cualquier otro id — widget local por tipo (IDG 1/2 son ahora `pb_guard`
  locales de verdad, ya no props inertes: todo control del modelo se
  construye igual, con o sin sim detrás).

Un control cockpit nuevo del catálogo sin cablear rompe
`test_every_cockpit_catalog_control_is_wired`, y un control del YAML sin
sitio rompe la cobertura de `test_layouts.py` — nada se pierde en silencio.

## Semántica de luces Korry (overhead)

| Estilo | Controles | Luz superior | Luz inferior |
|---|---|---|---|
| `auto_off` | BAT 1/2, BUS TIE, GALY & CAB | FAULT ámbar (`*_PB_HAS_FAULT`) | OFF blanca cuando el pb está suelto; apagada en AUTO |
| `on_off` | GEN 1/2, APU GEN, COMMERCIAL | FAULT ámbar | OFF blanca cuando suelto; apagada en ON |
| `on_avail` | EXT PWR | AVAIL verde (`ELEC_EXT_PWR_POTENTIAL_NORMAL` y pb no ON) | ON azul |
| `normal_altn` | AC ESS FEED | FAULT ámbar | ALTN blanca cuando pulsado (IS_NORMAL=0); apagada en NORMAL |
| `world` | GPU (ext_pwr_avail) | rótulo WORLD | SET verde |

**Gotcha**: `OVHD_ELEC_EXT_PWR_PB_IS_AVAILABLE` nunca sube en el build
headless; AVAIL usa el potencial de la GPU como señal honesta.

**Las luces del overhead son los flags CRUDOS de FBW, a propósito.** En cold &
dark el FAULT del AC ESS FEED luce encendido: su condición upstream es
`!ac_ess_bus_is_powered`, sin gating (la inhibición vivía en el FWC, que no
existe — D-014). El E/WD, en cambio, muestra el ECAM **gateado** por potencia.
Ver ambos a la vez es la historia de la Fase 2 en una pantalla: hardware crudo
arriba, detección honesta abajo. (En el avión real una cabina sin corriente no
ilumina anunciadores; modelar el bus de anunciadores sería fingir más de lo que
la sim sabe.)

**D-007 visible**: sin seeding, AC ESS FEED arranca en ALTN (IS_NORMAL lee 0) y
COMMERCIAL/GALY & CAB en OFF, aunque en MSFS arrancan en NORMAL/ON. Es el cold
& dark puro de este build, no un bug del panel.

## Synoptic ELEC

Render puro `render_elec_synoptic(SimState) -> rich.Text` con box-drawing,
convención del SD real: caja de bus **verde** si powered, **ámbar** si no; TRs
y fuentes verdes con salida normal, atenuadas si muertas. Los enlaces se pintan
verdes solo si **ambos extremos** están vivos — aproximación honesta del flujo,
no un ruteo fiel a contactores (p. ej. tras `fail elec.tr.1` con bus tie en
AUTO, DC 1 puede repowerse por el tie: el bus se ve verde y el TR atenuado, que
es exactamente lo que cuenta el sistema).

## E/WD

Alimentado por `read_ecam` (la promesa de la fila T del roadmap, cumplida al
mergear la Fase 2). Dos capas deliberadamente distintas:

- **Líneas ECAM** (arriba): lo que reporta la capa de detección — warnings en
  rojo, cautions en ámbar, ya ordenadas por severidad por el core. Las reglas
  `derived` llevan su marcador `(derived)` en tenue: es la frontera de D-014
  hecha visible. ECAM vacía en un avión sin corriente también es fiel (gate de
  potencia del core).
- **Ground truth inyectado** (abajo, tenue): qué fallos tiene activos el
  *escenario*. Un piloto nunca ve esto; existe porque la TUI es la cabina del
  operador del arnés, que es quien inyecta. (El agente MCP, en cambio, no lo ve
  ni aquí ni en ningún tool — D-016.)

`SimBridge` sigue degradando por `hasattr`: con un `a320_sim` anterior a la
Fase 2 el panel muestra solo el ground truth.

## Verificación

- `tui/tests/` (52 tests): el modelo parsea con ids únicos y defaults cold &
  dark (`test_model.py`); semántica por tipo del estado local
  (`test_cockpit_state.py`); anti-drift del wiring — toda var cableada existe
  en el registro, ningún control del catálogo queda sin cablear
  (`test_wiring.py`, más el manifest heredado en `test_manifest.py`); render
  puro de cada tipo de widget (`test_widget_render.py`); cobertura total de
  los layouts (`test_layouts.py`); y sinóptico (`test_synoptic_render.py`).
- e2e **commiteado como suite** (`test_app_elec_e2e.py`, `App.run_test()`):
  cold & dark → BAT 1/2 (DC BAT verde) → GPU (AVAIL) → BUS TIE + EXT PWR (red
  AC/DC verde) → `fail elec.tr.1` (caution en E/WD) → `unfail` restaura, todo
  por la ruta real de mensajes de los widgets; y el test de aislamiento local
  (guarda en dos pasos + selector ADIRS **sin** tocar `bridge.set`). BUS TIE
  es obligatorio: no hay seeding (D-007).
- Rendimiento medido (terminal 160×50, las 4 zonas montadas): ~705 nodos,
  arranque ~2.2 s (mount) + ~1 s de construir el `Sim`; refresh forzado
  ~10 ms. Fallback documentado si creciera: montaje diferido por cuadrante
  con `call_after_refresh` — **no** `ContentSwitcher` (decisión de UX: los 4
  cuadrantes visibles a la vez).
- Manual: `a320-tui` en **Windows Terminal** (conhost degrada box-drawing/ratón).

## Deuda (para la fase completa)

- Issues + EPIC `phase:T`/`area:tui` sin abrir.
- ~~`fail`/`unfail`/`failures` duplicados en `EmbeddedRepl`~~ — borrados al
  mergear la Fase 2; la gramática embebida hereda también `ecam`.
- ~~Tests Pilot e2e como suite~~ — commiteados (`test_app_elec_e2e.py`).
- Captura/screenshot en `docs/assets/` y fila del README con estado real.
- Páginas synoptic adicionales (HYD, etc.) cuando Fase 4 traiga los sistemas.
- Cuando un control local gradúe al catálogo de `core-rs` (Fase 4: APU ya
  cableado por LVAR crudo, hidráulico...), su entrada se añade/actualiza en
  `wiring.WIRING`; el anti-drift avisa de catálogo huérfano.
- Detalles de fidelidad del estado local: los switches spring-loaded
  (MAN V/S, INT/RAD, DOOR UNLOCK) no retornan solos a NEUTRAL; `LDG_ELEV` no
  tiene camino de vuelta al detent AUTO tras girarlo; los displays
  (MCDU/PFD/ND/ISIS) son placeholders oscuros.
- Re-sync de la spec externa (`Documents/a320`): copiar el YAML byte-idéntico
  y correr los tests; los números fijados (423, instancias) son el diff
  consciente.
