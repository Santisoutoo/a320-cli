# scenarios/ — the benchmark's dataset

One YAML per scenario, validated against
[`schema/scenario.schema.json`](schema/scenario.schema.json) plus live
catalog cross-checks by `a320_bench.scenario.load_scenario` (every control
name, failure id, start state and instructions profile must exist — a bad
reference fails at load time, never mid-LLM-run). Code lives in
[`bench/`](../bench/); this directory is data only.

## Anatomy of a scenario

- **`initial_state`** — a start state key (`cold-dark`, `apu-running`,
  `engines-running`) plus `world_controls`: `domain=world` state the harness
  pre-fixes (a plugged GPU, ambient conditions). The scenario fixes its
  world; the agent never actuates world controls as setup (Phase 3 closure
  note in [mcp/README.md](../mcp/README.md)).
- **`failures`** — catalog ids with an injection time (`after_setup_s`) or a
  state predicate (`when`), plus a settle time.
- **`expected_ecam`** — the run's **validity gate**, not the agent's task: if
  `must_appear` is not on the ECAM after injection + settle, the run aborts
  as `invalid_scenario` and is never scored. Only assert messages that are
  stable run-to-run: the vendor has real randomness (see the determinism
  decision in [docs/decisiones.md](../docs/decisiones.md)), and a caution
  that appears in *some* runs (e.g. `HYD ENG 2 PUMP FAULT` in the APU GEN
  scenario's probes) does not belong in the gate.
- **`ground_truth.procedure`** — an **ordered list of blocks**; blocks are
  strictly sequential, and within a block `ordered: true` demands sequence
  while its absence makes the actions an unordered set. This encodes the
  QRH's real dependencies (reset before alternate source) without
  over-penalizing interchangeable steps. `optional_actions` are permitted
  and never penalized; `forbidden_actions` carry a severity
  (`dangerous` / `anti_procedure`) for #20's penalty design.
- **`success`** — end-state predicates with tolerance (`eq/ne/gt/ge/lt/le/
  between`) plus `ecam_clear_of`. Never snapshot equality.
- **`budget`** — `max_tool_calls` and `max_sim_time_s` (counted from
  hand-over, not from setup).

## Citation policy (the paper depends on this)

Every scenario's `ground_truth.source` must carry:

- **`document`** — the procedure's identity: the Airbus FCOM/QRH section it
  encodes (e.g. `PRO-ABN-24 ELEC APU GEN FAULT`) plus any public secondary
  source actually consulted (the FlyByWire A32NX docs describe the *modeled*
  behavior and are URL-verifiable).
- **`revision`** and **`accessed`** — which version of the public source, and
  when. FCOM wording is copyrighted: we cite the procedure's identity and
  encode its *structure*, we do not reproduce its text.
- **`notes`** — the **fidelity boundary**: what was verified empirically
  against the vendor pin (commands, observed ECAM behavior, date, pin sha)
  and any divergence between the real procedure and what FBW models. A
  scenario whose real procedure depends on unmodeled behavior is invalid and
  gets excluded, not papered over (#19).

The vendor pin (`core-rs/vendor/aircraft`, currently `13bce4b`) is part of
the benchmark's identity: moving it re-opens every scenario's empirical
verification, and the scripted-procedure tests in
[`bench/tests/test_episode_scripted.py`](../bench/tests/test_episode_scripted.py)
are what catches that — each scenario ships with a script that flies its
procedure and must keep passing.

## Adding a scenario

1. Probe the failure empirically first (inject it by hand through the
   bindings; note which ECAM messages are stable across runs and what each
   pb actually does). The probe's findings go in `source.notes`.
2. Write the YAML; `load_scenario` must pass with catalog cross-checks.
3. Add a scripted-procedure test: the procedure must resolve the scenario
   (`all_passed: true`) and an ignore-everything script must not.
4. Cite the public sources per the policy above.
