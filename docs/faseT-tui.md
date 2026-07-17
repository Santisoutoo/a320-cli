# Fase T — TUI: cockpit en terminal sobre `a320_sim`

Nota de diseño de la fase T (track paralelo de frontend; decisión [D-018](decisiones.md)).
Estado: **PoC funcional** en `feat/tui-poc`. La fase completa (issues, tests Pilot
exhaustivos, captura en el README) queda pendiente de abrir su EPIC.

## Qué es

Un tercer frontend sobre la misma Control/Observe API: el REPL es la ventana
experta, el MCP la del LLM, y la TUI la ventana *visual* — ver los sistemas y
paneles del avión dentro de la terminal, en vivo. Sustituye conceptualmente al
`watch` del REPL (ANSI a pelo, lista plana) por una app Textual con paneles.

```
┌ StatusBar: t, ▶/⏸, xN, failures ──────────────────────────┐
├──────────────────────────┬────────────────────────────────┤
│ OVERHEAD · ELEC          │ SD · ELEC (synoptic)           │
│  [BAT 1][BAT 2][EXT PWR] │  buses verde/ámbar, TRs,       │
│  [GEN 1][BUS TIE][GEN 2] │  fuentes abajo                 │
│  — WORLD — [GPU]         ├────────────────────────────────┤
│                          │ E/WD (failures/warnings)       │
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
- **`manifest.py`**: la parte data-driven. `button_specs()` construye el
  overhead desde `list_controls()` + overlays de display (legend, semántica de
  luces Korry); un control sin overlay recibe un botón genérico, así los
  controles de Fase 4 aparecen sin tocar la TUI. `manifest_vars()` es el `get`
  selectivo del tick (~30 vars) — **nunca** `snapshot()`.
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

La geometría vive como **datos** en `manifest.py` (`PANEL_TOP_ROW`,
`PANEL_LEFT_STACK`, `PANEL_SOURCES_ROW`), con cuatro clases de slot:

- `catalog:` — controles del catálogo curado (los de siempre).
- `extra:` — hardware que FBW modela pero el catálogo aún no expone
  (AC ESS FEED, COMMERCIAL, GALY & CAB). Actúan por el camino de LVAR crudo de
  `sim.set` (D-008/D-009) — lo mismo que `set OVHD_...` en el REPL. Verificados
  contra el vendor (`a320_systems/src/electrical/mod.rs:283-286`) y contra el
  registro vivo (test `test_extra_panel_hardware_exists_in_the_registry`).
- `bat_display:` — los voltímetros reales entre los pb de batería
  (`ELEC_BAT_{1,2}_POTENTIAL`, vivos).
- `prop:` — posiciones del panel real que la sim **no** modela (IDG 1/2):
  inertes y visiblemente inertes; fingir función sería peor que el hueco.

El mímico de buses va **pintado** (verde estático), como en el avión: las
líneas del 35VU real no se encienden con el estado — la imagen viva es del
synoptic, y mantener los roles separados evita duplicar instrumentos.

Los controles de catálogo que la geometría no coloque caen en una sección
OTHER (la promesa data-driven sigue: un control de Fase 4 aparece sin tocar la
TUI, feo pero funcional, y el test `test_the_35vu_geometry_covers_todays_
cockpit_catalog` avisa para decidirle sitio).

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

- `tui/tests/`: anti-drift del manifest (toda var del manifest existe en el
  registro tras un tick — el gemelo Python del test del catálogo Rust), render
  del synoptic con estados sintéticos, y specs data-driven completos.
- Guion e2e (automatizado headless con `App.run_test()` durante la PoC):
  cold & dark → BAT 1/2 (DC BAT verde) → GPU (AVAIL) → BUS TIE + EXT PWR (red
  AC/DC completa verde en ~0.4 s sim) → `fail elec.tr.1` (caution en E/WD, TR 1
  atenuado) → `unfail` restaura. BUS TIE es obligatorio: no hay seeding (D-007).
- Manual: `a320-tui` en **Windows Terminal** (conhost degrada box-drawing/ratón).

## Deuda de la PoC (para la fase completa)

- Issues + EPIC `phase:T`/`area:tui` sin abrir; tests Pilot e2e como suite.
- ~~`fail`/`unfail`/`failures` duplicados en `EmbeddedRepl`~~ — borrados al
  mergear la Fase 2; la gramática embebida hereda también `ecam`.
- Captura/screenshot en `docs/assets/` y fila del README con estado real.
- Páginas synoptic adicionales (HYD, etc.) cuando Fase 4 traiga los sistemas
  (registro `SYNOPTIC_PAGES` ya extensible).
- Si AC ESS FEED / COMMERCIAL / GALY & CAB se catalogan en `core-rs` (Fase 4),
  mover sus overlays de `EXTRA_PANEL_SPECS` a `BUTTON_OVERLAYS` y borrar la
  entrada extra.
