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
- `a320_bench/episode.py` — episode runner (slice C, #70).
- `a320_bench/recorder.py` — JSONL trajectory writer (slice C, #70).
- `a320_bench/providers/` — agent adapters; `scripted` needs no LLM and is
  what CI runs (litellm adapter arrives in slice D, #71).

## Tests

```powershell
python -m pytest bench/tests -q     # no LLM, no network
```

## License

GPLv3 — drives the `a320_sim` extension, which links the vendored FlyByWire
crates and inherits their license.
