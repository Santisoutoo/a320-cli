# a320-tui — terminal cockpit (Fase T)

Textual TUI over the same headless core the REPL and the MCP server drive:
the **full A320 cockpit generated from a vendored YAML model** (~300
controls: overhead, glareshield, main panel, pedestal), an SD-style ELEC
synoptic, an E/WD warnings area and an embedded command line with the
REPL's grammar. Wired controls (ELEC, APU) actuate the sim; every other
control is still built and interactable on local state — pressing it,
opening its guard or moving its lever works, it just doesn't reach the
aircraft yet (the command log marks those `[local]`).

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

A 2×2 grid of independently scrollable quadrants (the real cockpit stacked
vertically is ~95 rows and fits no terminal):

- **OVERHEAD [F1]** (NW): aft + forward panels in three columns per the
  reference layout. The ELEC section is the wired 35VU — live battery
  voltmeters, the painted green bus mimic, and the sources row. Lights show
  FBW's *raw* pushbutton flags — the FWC-gated view is the E/WD's job.
- **GLARESHIELD · MAIN PANEL [F2]** (NE): FCU (push = managed, `p` = pull),
  EFIS ×2, master warn/caut, PFD/ND placeholders, ISIS, gear and brakes.
- **PEDESTAL [F3]** (SW): MCDU ×2 (key clusters as composite widgets),
  RMP/ACP, ECAM control panel, thrust quadrant, flaps, park brake, ATC/TCAS.
- **ELEC SD · E/WD [F4]** (SE): the synoptic (buses green when powered),
  the ECAM lines from `read_ecam` plus the injected ground truth in dim,
  and the `SCENARIO` section for world controls (GPU plugged) — not cockpit
  hardware.
- **Command line** (bottom): the REPL grammar (`set`, `get`, `step`, `env`,
  `fail`, `unfail`, `failures`, ...). `watch` and unbounded `run` are
  disabled — the TUI is already a live watch.

Keys: `F1`-`F4` focus a quadrant · `Tab` move focus · `Enter`/click actuate
(guarded buttons take two Enters; `Esc` closes the guard) · `[` / `]` turn
selectors, knobs, levers and key-cluster cursors · `space` pause/resume ·
`+`/`-` sim speed (up to x32) · `ctrl+q` quit.

## Quick demo (cold & dark → external power → failure)

1. Click `BAT 1`, `BAT 2` → watch `DC BAT` turn green in the synoptic.
2. Click `GPU` (SCENARIO, F4) → `EXT PWR` shows green `AVAIL`.
3. Click `BUS TIE` (mandatory: no seeding, D-007), then `EXT PWR` → the whole
   AC/DC network sequences to green in ~0.4 s of sim time.
4. Type `fail elec.tr.1` in the command line → amber entry in the E/WD and
   `TR 1` goes dim; `unfail elec.tr.1` restores it.

## Design notes

See `docs/faseT-tui.md` and decisions D-018/D-019 in `docs/decisiones.md`.
The rules that matter: `a320_sim.Sim` is unsendable, so all sim access stays
on the main event-loop thread (`SimBridge` asserts this); the tick reads a
selective `get` manifest (~30 vars), never `snapshot()`, and never refreshes
local widgets — only actuation does; the vendored YAML
(`a320_tui/model/a320-controls-model.yaml`) is the single source of truth
for the cockpit, and the anti-drift tests (`test_model`, `test_wiring`,
`test_layouts`) guarantee nothing is lost between the YAML, the sim catalog
and the zone layouts.
