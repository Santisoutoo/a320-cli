# a320-bench — Phase 5 benchmark harness

Code half of the benchmark (#19): scenario loading/validation, the episode
runner that sits an LLM in front of the benchmark MCP surface, and trajectory
recording. The data half — scenario YAMLs and their QRH-sourced ground truth —
lives in [`scenarios/`](../scenarios/).

## Install

```powershell
pip install -e bindings/   # a320_sim (not on PyPI)
pip install -e mcp/        # a320_mcp (START_STATES, create_server)
pip install -e bench/
```

## Layout

- `a320_bench/scenario.py` — YAML loader: jsonschema shape validation against
  [`scenarios/schema/scenario.schema.json`](../scenarios/schema/scenario.schema.json),
  then live cross-checks of every control name, failure id and start state
  against the core catalogs (a bad reference fails at load time, not
  mid-episode).
- `a320_bench/episode.py` — the episode runner. In-process and privileged: it
  owns the `Sim` (setup, world controls, failure injection, ground truth via
  `active_failures()`), while the agent talks to a benchmark-profile MCP
  server (no `inject_failure`/`clear_failure`, plus `report_done`) over the
  SDK's memory transport. Records every turn and tool call to JSONL with the
  simulated clock around each call; ends on `report_done`, budget exhaustion,
  two empty turns (one nudge), a provider error (partial trajectory kept), or
  a failed validity gate (`invalid_scenario`: the world never showed the ECAM
  the scenario promised, so there is nothing to score).
- `a320_bench/recorder.py` — JSONL trajectory writer/reader; the trajectory is
  self-contained (scenario embedded, tool surface, vendor pin, versions) so
  the #20 scorer never re-simulates.
- `a320_bench/providers/` — agent adapters; `scripted` needs no LLM and is
  what CI runs; `litellm_adapter` (pinned litellm, `[providers]` extra) is
  the canonical real-model path.
- `a320_bench/serve.py` — `a320-bench serve --scenario X --result out.json`:
  the same privileged setup + injection + validity gate, then the benchmark
  tool surface on **stdio** for an external MCP client that owns its own
  agent loop — above all `claude -p` on a Claude subscription. The harness's
  verdict lands in `--result` on shutdown. Demo/dev path: the trajectory is
  the client's transcript (`--output-format stream-json`), and the client's
  system prompt is a confound for model-vs-model baselines — those go
  through `a320-bench run`.

## Tests

```powershell
python -m pytest bench/tests -q     # no LLM, no network
```

## License

GPLv3 — drives the `a320_sim` extension, which links the vendored FlyByWire
crates and inherits their license.
