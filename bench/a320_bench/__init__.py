"""a320-bench: the Phase 5 benchmark harness (#19).

Data lives in ``scenarios/`` (YAML, one file per scenario); this package is
the code that loads it, runs an agent episode against the benchmark MCP
surface, and records the trajectory for #20 to score.

Imports are lazy (PEP 562) on purpose: the #20 scorer reads trajectories
(``read_trajectory``) without re-simulating, and must not need the compiled
``a320_sim`` binding just to import this package — only ``run_episode`` and
``load_scenario``'s catalog cross-checks pull it in.
"""

from typing import Any

_EXPORTS = {
    "EpisodeResult": "a320_bench.episode",
    "run_episode": "a320_bench.episode",
    "TrajectoryRecorder": "a320_bench.recorder",
    "read_trajectory": "a320_bench.recorder",
    "Scenario": "a320_bench.scenario",
    "ScenarioError": "a320_bench.scenario",
    "evaluate_predicate": "a320_bench.scenario",
    "load_scenario": "a320_bench.scenario",
}

__all__ = sorted(_EXPORTS)

__version__ = "0.1.0"


def __getattr__(name: str) -> Any:
    module_name = _EXPORTS.get(name)
    if module_name is None:
        raise AttributeError(f"module 'a320_bench' has no attribute '{name}'")
    import importlib

    return getattr(importlib.import_module(module_name), name)
