"""a320-bench: the Phase 5 benchmark harness (#19).

Data lives in ``scenarios/`` (YAML, one file per scenario); this package is
the code that loads it, runs an agent episode against the benchmark MCP
surface, and records the trajectory for #20 to score.
"""

from a320_bench.scenario import (
    Scenario,
    ScenarioError,
    evaluate_predicate,
    load_scenario,
)

__all__ = ["Scenario", "ScenarioError", "evaluate_predicate", "load_scenario"]

__version__ = "0.1.0"
