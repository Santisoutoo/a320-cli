"""a320-bench: the Phase 5 benchmark harness (#19).

Data lives in ``scenarios/`` (YAML, one file per scenario); this package is
the code that loads it, runs an agent episode against the benchmark MCP
surface, and records the trajectory for #20 to score.
"""

from a320_bench.episode import EpisodeResult, run_episode
from a320_bench.recorder import TrajectoryRecorder, read_trajectory
from a320_bench.scenario import (
    Scenario,
    ScenarioError,
    evaluate_predicate,
    load_scenario,
)

__all__ = [
    "EpisodeResult",
    "Scenario",
    "ScenarioError",
    "TrajectoryRecorder",
    "evaluate_predicate",
    "load_scenario",
    "read_trajectory",
    "run_episode",
]

__version__ = "0.1.0"
