"""``a320-bench``: run recorded benchmark episodes from the command line.

    a320-bench run --scenario scenarios/elec/apu_gen_fault.yaml \
                   --model anthropic/claude-opus-4-8 --runs 3 --out runs/

Each run gets a fresh Sim, a fresh benchmark-profile MCP server and its own
JSONL trajectory under ``<out>/<scenario_id>/``. The command needs the
``[providers]`` extra (litellm); everything else in the package runs without
it.
"""

import argparse
import asyncio
import json
import sys
from typing import Any

from a320_bench.episode import run_episode
from a320_bench.scenario import ScenarioError, load_scenario


def _positive_int(text: str) -> int:
    value = int(text)
    if value < 1:
        raise argparse.ArgumentTypeError(f"must be >= 1, got {value}")
    return value


def _sampling_dict(text: str) -> "dict[str, Any]":
    """Parse --sampling: must be a JSON object (litellm.completion kwargs)."""
    try:
        value = json.loads(text)
    except json.JSONDecodeError as exc:
        raise argparse.ArgumentTypeError(f"not valid JSON: {exc}") from exc
    if not isinstance(value, dict):
        raise argparse.ArgumentTypeError(
            f"must be a JSON object, got {type(value).__name__}"
        )
    return value


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="a320-bench",
        description="Phase 5 benchmark harness: recorded agent episodes over MCP.",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    run = sub.add_parser("run", help="run one scenario against a real model")
    run.add_argument("--scenario", required=True, help="path to a scenario YAML")
    run.add_argument(
        "--model",
        required=True,
        help="litellm model id, e.g. anthropic/claude-opus-4-8 or gpt-...",
    )
    run.add_argument(
        "--runs", type=_positive_int, default=1, help="episodes to run (default 1)"
    )
    run.add_argument("--out", default="runs", help="output directory (default runs/)")
    run.add_argument(
        "--sampling",
        type=_sampling_dict,
        default=None,
        help='JSON dict passed to litellm.completion verbatim, e.g. \'{"temperature": 0}\'',
    )
    return parser


def main(argv: "list[str] | None" = None) -> int:
    args = build_parser().parse_args(argv)

    # Imported here, not at module top: the CLI is the only piece that needs
    # litellm, and the error message tells the user exactly what to install.
    try:
        from a320_bench.providers.litellm_adapter import LiteLLMAdapter
    except ImportError as exc:
        print(f"a320-bench: {exc}", file=sys.stderr)
        return 2

    try:
        scenario = load_scenario(args.scenario)
    except ScenarioError as exc:
        print(f"a320-bench: {exc}", file=sys.stderr)
        return 2

    infra_failures = 0
    for i in range(args.runs):
        adapter = LiteLLMAdapter(args.model, sampling=args.sampling)
        result = asyncio.run(run_episode(scenario, adapter, args.out))
        if not result.valid:
            verdict = "INVALID"
        elif result.reason == "provider_error":
            verdict = "ERROR"
        else:
            verdict = "PASS" if result.all_passed else "FAIL"
        print(
            f"[{i + 1}/{args.runs}] {scenario.id} {verdict} "
            f"reason={result.reason} tool_calls={result.tool_calls_used} "
            f"sim_t={result.sim_time_end:.1f}s -> {result.trajectory_path}",
            file=sys.stderr,
        )
        if verdict in ("INVALID", "ERROR"):
            infra_failures += 1

    # Infrastructure problems deserve a red exit code: an invalid scenario
    # (the world never manifested the failure) or a provider error (bad key,
    # network down) — a paid batch of N broken runs must not end green. An
    # agent that failed the procedure is a *result*, not an error.
    return 1 if infra_failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
