# Fase T — TUI: cockpit en terminal sobre `a320_sim`

Nota de diseño de la fase T (track paralelo de frontend; decisión [D-014](decisiones.md)).
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

## Semántica de luces Korry (overhead)

| Estilo | Controles | Luz superior | Luz inferior |
|---|---|---|---|
| `auto_off` | BAT 1/2, BUS TIE | FAULT ámbar (`*_PB_HAS_FAULT`) | OFF blanca cuando el pb está suelto; apagada en AUTO |
| `on_off` | GEN 1/2, APU GEN | FAULT ámbar | OFF blanca cuando suelto; apagada en ON |
| `on_avail` | EXT PWR | AVAIL verde (`ELEC_EXT_PWR_POTENTIAL_NORMAL` y pb no ON) | ON azul |
| `world` | GPU (ext_pwr_avail) | rótulo WORLD | SET verde |

**Gotcha**: `OVHD_ELEC_EXT_PWR_PB_IS_AVAILABLE` nunca sube en el build
headless; AVAIL usa el potencial de la GPU como señal honesta.

## Synoptic ELEC

Render puro `render_elec_synoptic(SimState) -> rich.Text` con box-drawing,
convención del SD real: caja de bus **verde** si powered, **ámbar** si no; TRs
y fuentes verdes con salida normal, atenuadas si muertas. Los enlaces se pintan
verdes solo si **ambos extremos** están vivos — aproximación honesta del flujo,
no un ruteo fiel a contactores (p. ej. tras `fail elec.tr.1` con bus tie en
AUTO, DC 1 puede repowerse por el tie: el bus se ve verde y el TR atenuado, que
es exactamente lo que cuenta el sistema).

## E/WD

Placeholder honesto mientras no exista `read_ecam` (#15): lista los failures
*inyectados* (ground truth del catálogo, no detección del FWC) como cautions
ámbar, con cabecera que lo dice. `SimBridge` ya detecta capacidades por
`hasattr` (funciona con un `a320_sim` sin failures); cuando el `read_ecam` de
Fase 2 llegue al binding instalado, solo cambia el armado de líneas del panel.

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
- `fail`/`unfail`/`failures` duplicados en `EmbeddedRepl` hasta que feat/14
  merge (borrarlos entonces).
- Captura/screenshot en `docs/assets/` y fila del README con estado real.
- Páginas synoptic adicionales (HYD, etc.) cuando Fase 4 traiga los sistemas
  (registro `SYNOPTIC_PAGES` ya extensible).
