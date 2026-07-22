# a320-mcp — the LLM's window onto the aircraft

An MCP server exposing the headless A320 systems core, so an LLM agent can fly
the closed loop: observe the ECAM, reason over the procedure, act on the
switches, advance time, observe again.

Same core the [CLI](../cli/README.md) drives — two frontends, one API. Every
tool here is a 1:1 mapping onto `a320_sim.Sim`; there is no simulation logic in
this package.

## Install

Needs Python **≥ 3.10** (the MCP SDK's floor; the rest of the repo is ≥ 3.9).
Install the bindings first — they are not on PyPI:

```powershell
pip install -e bindings/
pip install -e mcp/
```

## Run

```powershell
a320-mcp                            # cold & dark (default)
a320-mcp --start apu-running        # APU started and feeding the AC network
a320-mcp --start engines-running    # both engines at idle powering everything
```

The server speaks **stdio**, so you don't normally run it by hand — a client
launches it and talks over the pipe.

`--start apu-running` takes ~60 s of *simulated* time at boot (a few seconds of
wall clock) to spin the APU up before serving; `--start engines-running` runs
the full cold & dark → engines running sequence (~6 min simulated: APU, APU
bleed, both engine starts, APU shutdown) and hands over a healthy aircraft on
its engine generators with a clean ECAM. The scenario is the harness's job, not
the agent's.

## Point a client at it

The repo ships a [`.mcp.json`](../.mcp.json) that registers this server:

```json
{
  "mcpServers": {
    "a320": {
      "command": "a320-mcp",
      "args": ["--start", "apu-running"]
    }
  }
}
```

The client spawns `a320-mcp`, so **it has to be on the PATH of the process that
launches the client** — activate the virtualenv you installed it into before
starting the client, or point `command` at the executable directly
(`.venv/Scripts/a320-mcp.exe` on Windows, `.venv/bin/a320-mcp` elsewhere).

MCP servers are read at client startup, so a client that was already running
won't see it until you restart it.

Then hand it the scenario:

> The APU generator just failed. Deal with it.

The agent has the ECAM and the switches — and nothing that tells it what broke.

## Tools

| Tool | What it does |
|---|---|
| `read_ecam` | Active warnings and cautions, worst first — the agent's primary observation |
| `read_state` | Read named state variables (takes a list) |
| `snapshot` | Discover state variables by name filter |
| `list_controls` | The curated cockpit controls you can actuate |
| `list_failures` | The catalog of what *can* break |
| `set_control` | Flip a switch or pushbutton |
| `advance` | Advance simulated time — **nothing takes effect until you do** |
| `inject_failure` | Break something (reversibly) |
| `clear_failure` | Repair it |

Valid control names and failure ids are baked into the tool schemas as enums,
generated from the catalogs — the model cannot name something that isn't there.

**Two tools deliberately do not exist.** The core can report which failures are
active and can list every variable; neither is exposed. The first would hand the
agent the answer it is supposed to diagnose from the ECAM; the second would bury
the context window under hundreds of names. See D-016 in
[docs/decisiones.md](../docs/decisiones.md).

## Worked example: the agent loop

What a client sees driving the `apu-running` scenario:

```text
> read_ecam
  []                                      # healthy: the APU feeds the network

  (a failure is injected by the harness)

> read_ecam
  [{"message": "AC ESS BUS FAULT", "severity": "caution", "system": "ELEC", "source": "vendor_flag"},
   {"message": "APU GEN FAULT",    "severity": "caution", "system": "ELEC", "source": "vendor_flag"}]

> snapshot "_BUS_IS_POWERED"
  {"ELEC_AC_1_BUS_IS_POWERED": 0.0,       # the whole AC network is gone...
   "ELEC_AC_2_BUS_IS_POWERED": 0.0,
   "ELEC_AC_ESS_BUS_IS_POWERED": 0.0,
   "ELEC_DC_1_BUS_IS_POWERED": 0.0,       # ...and DC 1/2 with it: they feed via the TRs
   "ELEC_DC_2_BUS_IS_POWERED": 0.0,
   "ELEC_DC_BAT_BUS_IS_POWERED": 1.0,     # batteries hold the essential DC —
   "ELEC_DC_ESS_BUS_IS_POWERED": 1.0}     # which is why the ECAM is still readable

> set_control apu_gen 0                   # the faulty source out of the loop
> set_control ext_pwr_avail 1             # ask for a GPU
> set_control ext_pwr 1                   # put it on the network
> advance 3
> read_ecam
  []                                      # clear: AC 1/2/ESS and DC 1/2 all back
```

Losing the APU generator raises *two* cautions — the source fault and the
downstream AC ESS bus — because it was the only AC source. That cascade is the
scenario, and dealing with it is the task.

The engine generators are no help here (no engines running), so on the ground the
answer is external power. Note `ext_pwr_avail` is a `world` control, not a cockpit
one: a real crew asks for a ground power unit, they don't plug it in themselves.
A Phase 5 scenario should pre-set its world state rather than hand it to the agent
— see the Phase 3 closure note in [docs/decisiones.md](../docs/decisiones.md).

## Tests

They spawn the server as a subprocess and talk to it over the real stdio
protocol, so what is checked is what an agent would see:

```powershell
python mcp/tests/test_server.py     # or: pytest mcp/tests/
```

## License

GPLv3 — the server drives the `a320_sim` extension, which links the vendored
FlyByWire crates and inherits their license.
