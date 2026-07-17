# a320-tui — terminal cockpit (Fase T)

Textual TUI over the same headless core the REPL and the (future) MCP server
drive: an interactive ELEC overhead panel, an SD-style ELEC synoptic, an E/WD
warnings area and an embedded command line with the REPL's grammar.

## Install & run

In a venv where the `a320_sim` extension is already installed
(`pip install -e bindings/`):

```powershell
pip install -e cli/ -e tui/
a320-tui           # or: python -m a320_tui
a320-cli           # also launches the TUI when a320-tui is installed
a320-cli --repl    # the classic terminal REPL
```

Use **Windows Terminal** (or any modern terminal): classic conhost degrades
colors, box-drawing and mouse support.

## Layout & keys

- **OVERHEAD · ELEC (35VU)** (left): the real A320 panel geometry — COMMERCIAL
  and GALY & CAB stacked far left, live battery voltmeters between the BAT 1/2
  pushbuttons, AC ESS FEED, the painted green bus mimic, and the sources row
  IDG 1 · GEN 1 · APU GEN · BUS TIE · EXT PWR · GEN 2 · IDG 2. Click (or focus
  + Enter) to actuate. IDG positions are inert (the sim does not model them,
  and they say so). Lights show FBW's *raw* pushbutton flags — the FWC-gated
  view is the E/WD's job. The `WORLD` section holds scenario controls (GPU
  plugged), not cockpit hardware.
- **SD · ELEC** (right): buses green when powered, amber when not; TRs and
  sources green when their output is normal. Links are green only when both
  ends are alive (an approximation of flow, not contactor-accurate routing).
- **E/WD**: the ECAM lines from `read_ecam` (red warnings, amber cautions,
  severity-ordered; `derived` rules carry their marker), plus the injected
  ground truth in dim below — the harness operator sees both; the MCP agent
  sees only the ECAM.
- **Command line** (bottom): the REPL grammar (`set`, `get`, `step`, `env`,
  `fail`, `unfail`, `failures`, ...). `watch` and unbounded `run` are
  disabled — the TUI is already a live watch.

Keys: `space` pause/resume · `+`/`-` sim speed (up to x32) · `Tab` move
focus · `ctrl+q` quit.

## Quick demo (cold & dark → external power → failure)

1. Click `BAT 1`, `BAT 2` → watch `DC BAT` turn green in the synoptic.
2. Click `GPU` (WORLD) → `EXT PWR` shows green `AVAIL`.
3. Click `BUS TIE` (mandatory: no seeding, D-007), then `EXT PWR` → the whole
   AC/DC network sequences to green in ~0.4 s of sim time.
4. Type `fail elec.tr.1` in the command line → amber entry in the E/WD and
   `TR 1` goes dim; `unfail elec.tr.1` restores it.

## Design notes

See `docs/faseT-tui.md` and decision D-018 in `docs/decisiones.md`. The rules
that matter: `a320_sim.Sim` is unsendable, so all sim access stays on the main
event-loop thread (`SimBridge` asserts this); the tick reads a selective `get`
manifest (~30 vars), never `snapshot()`.
