"""The episode runner: one scenario, one agent, one recorded trajectory.

Architecture (see the Phase 5 decisions in docs/decisiones.md): the runner is
**in-process and privileged**. It owns the ``a320_sim.Sim`` and does the
harness work with direct calls — start state, world controls, failure
injection, ground-truth verification via ``active_failures()``/``snapshot()``
— none of which the agent can see. The agent gets a real MCP session over the
SDK's memory transport (``create_connected_server_and_client_session``, which
accepts the FastMCP instance and initializes the session itself), speaking to
a server built with the **benchmark tool profile**: no ``inject_failure``, no
``clear_failure``, plus ``report_done`` as the end-of-episode channel.

Everything runs on one thread of one event loop — the binding is
``unsendable`` (D-010) and both the server's tools and the runner's direct
calls touch the same Sim.

Episode end (``final.reason``):
- ``agent_done`` — the agent called report_done.
- ``end_turn_without_done`` — two consecutive turns without tool calls (the
  runner nudges once).
- ``budget_tool_calls`` / ``budget_sim_time`` — budget exhausted.
- ``provider_error`` — the provider raised; the partial trajectory is kept.
- ``invalid_scenario`` — the validity gate failed: the world never showed the
  ECAM the scenario promised, so there is nothing scoreable.
"""

import subprocess
import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from importlib import metadata
from pathlib import Path
from typing import Any

import a320_sim
from a320_mcp.server import INSTRUCTIONS_PROFILES, START_STATES, create_server
from mcp.shared.memory import create_connected_server_and_client_session
from mcp.types import CallToolResult

from a320_bench.providers.base import ProviderAdapter, ToolResult, Turn
from a320_bench.recorder import TRAJECTORY_SCHEMA_VERSION, TrajectoryRecorder
from a320_bench.scenario import Scenario, evaluate_predicate

SETTLE_RATE_HZ = 5.0
NUDGE = (
    "You made no tool call. Continue with the procedure, or call report_done "
    "if you consider the situation handled."
)


@dataclass(frozen=True)
class EpisodeResult:
    """Summary of one finished episode; the JSONL at `trajectory_path` has the detail.

    `reason` is one of the ``final.reason`` values in the module docstring.
    `valid` is False only for ``invalid_scenario``, in which case `all_passed`
    is None (the agent was never consulted, there is nothing to grade);
    otherwise `all_passed` mirrors the harness's success evaluation.
    """

    run_id: str
    trajectory_path: Path
    reason: str
    valid: bool
    all_passed: "bool | None"
    tool_calls_used: int
    sim_time_end: float


def _vendor_pin() -> str:
    """Short sha of the vendored FBW submodule — part of the benchmark's identity."""
    from a320_bench.scenario import REPO_ROOT

    try:
        out = subprocess.run(
            ["git", "-C", str(REPO_ROOT), "rev-parse", "HEAD:core-rs/vendor/aircraft"],
            capture_output=True,
            text=True,
            timeout=10,
            check=True,
        )
        return out.stdout.strip()[:9]
    except (OSError, subprocess.SubprocessError):
        return "unknown"


def _versions() -> dict[str, str]:
    versions = {"trajectory_schema": str(TRAJECTORY_SCHEMA_VERSION)}
    for dist in ("a320-sim", "a320-mcp", "a320-bench", "mcp"):
        try:
            versions[dist] = metadata.version(dist)
        except metadata.PackageNotFoundError:  # pragma: no cover - broken env
            versions[dist] = "unknown"
    return versions


def _ecam_messages(sim: "a320_sim.Sim") -> list[str]:
    return [w["message"] for w in sim.read_ecam()]


def _setup(sim: "a320_sim.Sim", scenario: Scenario) -> None:
    """World first, then the start state, then cockpit overrides.

    World controls go before the start state on purpose: the world (a plugged
    GPU, ambient state) exists while the aircraft is being prepared, exactly
    as it would for a real turnaround.
    """
    for name, value in scenario.initial_state.world_controls.items():
        sim.set(name, value)
    START_STATES[scenario.initial_state.start](sim)
    if scenario.initial_state.set_controls:
        for name, value in scenario.initial_state.set_controls.items():
            sim.set(name, value)
        sim.run(2.0, SETTLE_RATE_HZ)


def _inject(sim: "a320_sim.Sim", scenario: Scenario) -> "str | None":
    """Run the injection schedule. Returns an error string if it could not."""
    for failure in scenario.failures:
        if failure.after_setup_s is not None:
            if failure.after_setup_s > 0:
                sim.run(failure.after_setup_s, SETTLE_RATE_HZ)
        else:  # when-predicate (schema guarantees exactly one of the two)
            waited = 0.0
            pred = failure.when
            while not evaluate_predicate(pred, sim.get([pred.var])[pred.var]):
                sim.run(1.0, SETTLE_RATE_HZ)
                waited += 1.0
                if waited > scenario.budget.max_sim_time_s:
                    return (
                        f"injection trigger for '{failure.id}' never held: "
                        f"{pred.var} {pred.op} did not occur within "
                        f"{scenario.budget.max_sim_time_s:g}s"
                    )
        sim.inject_failure(failure.id)
        sim.run(failure.settle_s, SETTLE_RATE_HZ)
    return None


def _validity_gate(sim: "a320_sim.Sim", scenario: Scenario) -> dict[str, Any]:
    messages = _ecam_messages(sim)
    missing = [m for m in scenario.expected_ecam.must_appear if m not in messages]
    intruding = [m for m in scenario.expected_ecam.must_not_appear if m in messages]
    return {
        "passed": not missing and not intruding,
        "ecam": messages,
        "missing": missing,
        "must_not_appear_present": intruding,
    }


def _evaluate_success(sim: "a320_sim.Sim", scenario: Scenario) -> dict[str, Any]:
    checks = []
    for pred in scenario.success.final_state:
        try:
            value = sim.get([pred.var])[pred.var]
            passed = evaluate_predicate(pred, value)
            checks.append(
                {"var": pred.var, "op": pred.op, "value": value, "passed": passed}
            )
        except a320_sim.SimError as exc:
            checks.append({"var": pred.var, "op": pred.op, "error": str(exc), "passed": False})
    messages = _ecam_messages(sim)
    ecam_checks = [
        {"message": m, "clear": m not in messages} for m in scenario.success.ecam_clear_of
    ]
    all_passed = all(c["passed"] for c in checks) and all(c["clear"] for c in ecam_checks)
    return {"final_state": checks, "ecam_clear_of": ecam_checks, "all_passed": all_passed}


def _tool_result_text(result: CallToolResult) -> str:
    return " ".join(
        c.text for c in result.content if getattr(c, "type", None) == "text"
    )


async def run_episode(
    scenario: Scenario,
    adapter: ProviderAdapter,
    out_dir: "str | Path",
    *,
    run_id: "str | None" = None,
) -> EpisodeResult:
    """Run one recorded episode. Returns the result; the JSONL is the artifact."""
    run_id = run_id or (
        f"{datetime.now(timezone.utc):%Y%m%dT%H%M%SZ}-{uuid.uuid4().hex[:6]}"
    )
    out_path = Path(out_dir) / scenario.id / f"{run_id}.jsonl"

    sim = a320_sim.Sim()

    with TrajectoryRecorder(out_path) as rec:
        # --- harness work: setup, injection, validity gate -------------------
        # A harness crash is a bug in us, not in the agent: leave evidence in
        # the trajectory instead of an empty orphan file, then re-raise.
        try:
            _setup(sim, scenario)
            injection_error = _inject(sim, scenario)
        except Exception as exc:
            rec.write("harness_error", stage="setup", error=repr(exc))
            raise
        gate = (
            {"passed": False, "error": injection_error}
            if injection_error
            else _validity_gate(sim, scenario)
        )
        t0 = sim.sim_time()

        instructions = INSTRUCTIONS_PROFILES[scenario.instructions_profile]
        server = create_server(sim, instructions=instructions, profile="benchmark")

        async with create_connected_server_and_client_session(server) as session:
            tools = (await session.list_tools()).tools
            tool_schemas = [
                {
                    "name": t.name,
                    "description": t.description or "",
                    "inputSchema": t.inputSchema,
                }
                for t in tools
            ]

            rec.write(
                "meta",
                run_id=run_id,
                ts_utc=datetime.now(timezone.utc).isoformat(),
                scenario=scenario.raw,
                scenario_path=str(scenario.path),
                adapter=adapter.info,
                instructions_profile=scenario.instructions_profile,
                tool_surface=sorted(t.name for t in tools),
                vendor_pin=_vendor_pin(),
                versions=_versions(),
                seed=None,  # reserved: vendor RNG is not seedable today (see decisiones.md)
            )
            rec.write(
                "setup",
                sim_time=t0,
                validity_gate=gate,
                active_failures=sim.active_failures(),
                snapshot=sim.snapshot(),
            )

            if not gate["passed"]:
                rec.write(
                    "final",
                    reason="invalid_scenario",
                    sim_time=sim.sim_time(),
                    tool_calls_used=0,
                    success_eval=None,
                )
                return EpisodeResult(
                    run_id=run_id,
                    trajectory_path=out_path,
                    reason="invalid_scenario",
                    valid=False,
                    all_passed=None,
                    tool_calls_used=0,
                    sim_time_end=sim.sim_time(),
                )

            # --- the agent loop ---------------------------------------------
            reason = None
            done_payload: "dict[str, Any] | None" = None
            calls_used = 0
            nudged = False
            step = 0

            def over_budget() -> "str | None":
                if calls_used >= scenario.budget.max_tool_calls:
                    return "budget_tool_calls"
                if sim.sim_time() - t0 >= scenario.budget.max_sim_time_s:
                    return "budget_sim_time"
                return None

            try:
                turn: Turn = adapter.start(
                    instructions=instructions,
                    tools=tool_schemas,
                    user_message=scenario.task_prompt,
                )
                while reason is None:
                    step += 1
                    rec.write(
                        "assistant",
                        step=step,
                        text=turn.text,
                        stop_reason=turn.stop_reason,
                    )

                    if not turn.tool_calls:
                        if nudged:
                            reason = "end_turn_without_done"
                            break
                        nudged = True
                        # Materialized in the trajectory: the scorer must see
                        # every message the agent saw, not infer it.
                        rec.write("nudge", step=step, text=NUDGE)
                        turn = adapter.next([], nudge=NUDGE)
                        continue

                    results: list[ToolResult] = []
                    for call in turn.tool_calls:
                        sim_before = sim.sim_time()
                        wall_before = datetime.now(timezone.utc)
                        try:
                            outcome = await session.call_tool(call.name, call.args)
                            text = _tool_result_text(outcome)
                            is_error = bool(outcome.isError)
                            structured = outcome.structuredContent
                        except Exception as exc:  # protocol-level failure
                            text, is_error, structured = str(exc), True, None
                        calls_used += 1
                        rec.write(
                            "tool_call",
                            step=step,
                            call_index=len(results),
                            name=call.name,
                            args=call.args,
                            result=structured if structured is not None else text,
                            is_error=is_error,
                            sim_time_before=sim_before,
                            sim_time_after=sim.sim_time(),
                            wall_ms=int(
                                (datetime.now(timezone.utc) - wall_before).total_seconds() * 1000
                            ),
                        )
                        results.append(ToolResult(call=call, content=text, is_error=is_error))

                        if call.name == "report_done" and not is_error:
                            reason = "agent_done"
                            done_payload = call.args
                            break
                        budget_hit = over_budget()
                        if budget_hit:
                            reason = budget_hit
                            break

                    if reason is None:
                        turn = adapter.next(results)
            except Exception as exc:  # provider blew up: keep the partial trajectory
                reason = "provider_error"
                rec.write("provider_error", step=step, error=repr(exc))

            # --- the harness judges the end state ----------------------------
            success_eval = _evaluate_success(sim, scenario)
            rec.write(
                "final",
                reason=reason,
                done_payload=done_payload,
                sim_time=sim.sim_time(),
                tool_calls_used=calls_used,
                ecam=_ecam_messages(sim),
                active_failures=sim.active_failures(),
                snapshot=sim.snapshot(),
                success_eval=success_eval,
            )

    return EpisodeResult(
        run_id=run_id,
        trajectory_path=out_path,
        reason=reason,
        valid=True,
        all_passed=success_eval["all_passed"],
        tool_calls_used=calls_used,
        sim_time_end=sim.sim_time(),
    )
