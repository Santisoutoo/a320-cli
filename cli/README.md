# a320-cli — human REPL

The terminal frontend onto the headless A320 systems core. One of the two windows
onto the same control/observe API (the other is the MCP server); this one is for a
**human** to flip switches, read buses, advance time and watch the electrical
network come alive.

It contains no simulation logic: every command maps 1:1 onto the `a320_sim.Sim`
API exposed by the [`bindings/`](../bindings) PyO3 extension.

## Install

The CLI drives the `a320_sim` extension, so build the bindings first, then install
the CLI into the same environment:

```powershell
python -m venv .venv
.\.venv\Scripts\Activate.ps1

pip install -e bindings/    # builds the Rust core into the a320_sim extension
pip install -e cli/         # installs the a320-cli console script
```

On Windows, `pip install -e cli/` also pulls `pyreadline3`, which provides the
`readline` module `cmd.Cmd` uses for tab-completion. On Linux/macOS CPython ships
`readline`, so nothing extra is needed.

## Run

Either the console script or the module form works:

```powershell
a320-cli                    # console script (after pip install -e cli/)
python -m a320_cli          # module form (run from cli/, or after install)
```

## Commands

| Command | What it does |
|---|---|
| `set <control> <value>` | Actuate a control. `<control>` is a friendly name (`bat_1`, `ext_pwr`, `bus_tie`…) or a raw LVAR. `<value>` is a number or a friendly boolean: `on`/`off`, `true`/`false`, `yes`/`no`, `auto` (auto = on). **Tab-completes control names.** |
| `get <var> [<var> …]` | Read one or more variables. Tab-completes variable names. |
| `step [<ms>]` | Advance one tick of `<ms>` ms (default 1000). |
| `run <seconds> [<rate_hz>]` | Advance `<seconds>` at `<rate_hz>` (default 5 Hz), as fast as it computes. |
| `env <alt_ft> <ias_kt> <oat_c> <qnh_hpa>` | Set the outside world (the `UpdateContext`). |
| `snapshot [<substr>]` | Full state dump, optionally filtered to names containing `<substr>`. |
| `controls` | List the curated, actuable controls (name, type, valid values, LVAR). |
| `vars [<substr>]` | List known variable names, optionally filtered. |
| `watch <var> [<var> …]` | **Live view**: advance at ~5 Hz and re-render the vars in place until `Ctrl+C`. |
| `help [<command>]` | Command list, or detailed help for one command. |
| `quit` / `exit` / `Ctrl+D` | Leave the REPL. |

Errors — an unknown control, a value out of range, a bad argument — print a
one-line message and return to the prompt. A traceback is never shown.

## Worked example: cold & dark → battery → external power

This is the Phase-1 target scenario. Start `a320-cli` and type:

```text
a320 [t=   0.0s]> step 1000
  stepped 1000 ms  (t=1.000s)

a320 [t=   1.0s]> set bat_1 on
  bat_1 <- 1
a320 [t=   1.0s]> set bat_2 on
  bat_2 <- 1

a320 [t=   1.0s]> watch ELEC_DC_BAT_BUS_IS_POWERED ELEC_AC_1_BUS_IS_POWERED
watching 2 var(s) at 5 Hz - Ctrl+C to stop
* ELEC_DC_BAT_BUS_IS_POWERED   1
  ELEC_AC_1_BUS_IS_POWERED     0
  t=    2.00s
```

The `*` marks a powered bus. Around `t=2.0 s` the **DC BAT bus flips from 0 to 1**
as the battery contactors close — AC stays dead, there is no AC source yet. Press
`Ctrl+C` to stop watching (the sim keeps its state).

Now bring in external power. The **bus tie must be in AUTO**: there is no state
seeding ([D-007](../docs/decisiones.md)), so the pushbutton starts OFF and the AC
tie contactors would stay open otherwise.

```text
a320 [t=   3.2s]> set bus_tie auto
  bus_tie <- 1
a320 [t=   3.2s]> set ext_pwr_avail 1
  ext_pwr_avail <- 1
a320 [t=   3.2s]> set ext_pwr on
  ext_pwr <- 1

a320 [t=   3.2s]> watch ELEC_AC_1_BUS_IS_POWERED ELEC_AC_2_BUS_IS_POWERED ELEC_DC_1_BUS_IS_POWERED ELEC_DC_2_BUS_IS_POWERED
watching 4 var(s) at 5 Hz - Ctrl+C to stop
* ELEC_AC_1_BUS_IS_POWERED     1
* ELEC_AC_2_BUS_IS_POWERED     1
* ELEC_DC_1_BUS_IS_POWERED     1
* ELEC_DC_2_BUS_IS_POWERED     1
  t=    3.60s
```

Around `t=3.6 s` the **whole AC network wakes up** — AC 1/2 powered through the
bus tie contactors, TR 1/2 giving normal potential, DC 1/2 fed via the TRs. You
have taken the aircraft from cold & dark to a fully powered network from a
terminal.

> Tip: when stdout is a real terminal, `watch` redraws the same lines in place.
> When piped (for capture/automation) it prints one compact line per refresh so
> the transitions read cleanly in a log.
