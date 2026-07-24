"""``a320-bench serve``: a scenario served over stdio, benchmark profile.

The bridge for agents the runner cannot drive as a ProviderAdapter — above
all **Claude Code on a subscription** (``claude -p`` is its own agent loop
with its own MCP client, so it connects here instead of being called by
``run_episode``). This does the privileged half of an episode in-process
(setup, world controls, injection, validity gate), then serves the benchmark
tool surface on stdio; on shutdown it writes the harness's own success
evaluation to ``--result`` so the outer process can judge the run.

    a320-bench serve --scenario scenarios/elec/apu_gen_fault.yaml \
                     --result result.json

    claude -p "<task prompt>" --mcp-config <cfg pointing at the line above> ...

Trade-off, stated honestly: unlike ``a320-bench run``, the trajectory is NOT
recorded here (the client owns the loop; use its transcript, e.g.
``claude -p --output-format stream-json``), and the client's own system
prompt is a confound for model-vs-model comparisons — this is the demo/dev
path, not the canonical baseline path.

stdout is the MCP transport: everything diagnostic goes to stderr.
"""

import atexit
import json
import sys
from pathlib import Path

import a320_sim
from a320_mcp.server import INSTRUCTIONS_PROFILES, create_server

from a320_bench.episode import _evaluate_success, _inject, _setup, _validity_gate
from a320_bench.scenario import Scenario


def serve_scenario(scenario: Scenario, *, result_path: "Path | None" = None) -> int:
    """Set up, inject, gate — then serve on stdio until the client hangs up."""
    sim = a320_sim.Sim()
    print(f"serve: preparing '{scenario.id}' ({scenario.initial_state.start})...", file=sys.stderr)
    _setup(sim, scenario)
    error = _inject(sim, scenario)
    gate = {"passed": False, "error": error} if error else _validity_gate(sim, scenario)
    print(f"serve: validity gate: {gate}", file=sys.stderr)
    if not gate["passed"]:
        print("serve: invalid scenario, refusing to serve", file=sys.stderr)
        return 1

    if result_path is not None:

        def dump_result() -> None:
            result = {
                "scenario": scenario.id,
                "success_eval": _evaluate_success(sim, scenario),
                "final_ecam": [w["message"] for w in sim.read_ecam()],
                "active_failures": sim.active_failures(),
                "sim_time": sim.sim_time(),
            }
            result_path.parent.mkdir(parents=True, exist_ok=True)
            result_path.write_text(json.dumps(result, indent=2), encoding="utf-8")
            print(f"serve: result written to {result_path}", file=sys.stderr)

        atexit.register(dump_result)

    server = create_server(
        sim,
        instructions=INSTRUCTIONS_PROFILES[scenario.instructions_profile],
        profile="benchmark",
    )
    print(f"serve: benchmark profile on stdio (t={sim.sim_time():.1f}s)", file=sys.stderr)
    server.run()
    return 0
