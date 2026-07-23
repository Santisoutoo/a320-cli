"""End-to-end episode tests with a scripted agent — no LLM, no network (#70).

The APU GEN scenario is driven twice: once flying the procedure (must pass)
and once ignoring it (must fail). That asymmetry is the point: a harness that
says yes to everything measures nothing.
"""

import asyncio
import tempfile
from pathlib import Path

from a320_bench import load_scenario, read_trajectory, run_episode
from a320_bench.providers import ScriptedAdapter
from a320_bench.scenario import REPO_ROOT

FIRST_SCENARIO = REPO_ROOT / "scenarios" / "elec" / "apu_gen_fault.yaml"

# The ground-truth procedure, as tool calls: observe, reset attempt (fails),
# faulty source off + external power on, verify, report.
PROCEDURE_SCRIPT = [
    [("read_ecam", {})],
    [("set_control", {"control": "apu_gen", "value": 0}), ("advance", {"seconds": 2})],
    [("set_control", {"control": "apu_gen", "value": 1}), ("advance", {"seconds": 2})],
    [("read_ecam", {})],  # caution back: reset unsuccessful
    [
        ("set_control", {"control": "apu_gen", "value": 0}),
        ("set_control", {"control": "ext_pwr", "value": 1}),
        ("advance", {"seconds": 5}),
    ],
    [("read_ecam", {})],
    [("report_done", {"diagnosis": "APU generator failed; reset unsuccessful",
                      "actions_summary": "APU GEN off/on reset, then APU GEN off and EXT PWR on"})],
]


def _run(script, scenario_path=FIRST_SCENARIO, **kwargs):
    scenario = load_scenario(scenario_path)
    with tempfile.TemporaryDirectory() as tmp:
        result = asyncio.run(
            run_episode(scenario, ScriptedAdapter(script), tmp, **kwargs)
        )
        return result, read_trajectory(result.trajectory_path)


def test_scripted_procedure_resolves_the_scenario():
    """A script that flies the procedure ends agent_done with all checks green.

    This is also the executable smoke test of the scenario's ground truth: if
    the vendor pin moves and the procedure stops working, this fails.
    """
    result, records = _run(PROCEDURE_SCRIPT)

    assert result.reason == "agent_done"
    assert result.valid
    assert result.all_passed is True

    final = records[-1]
    assert final["type"] == "final"
    assert final["success_eval"]["all_passed"] is True
    assert final["done_payload"]["diagnosis"].startswith("APU generator")
    assert "APU GEN FAULT" not in final["ecam"]
    # The failure is still injected — the agent managed it, it did not repair it.
    assert final["active_failures"] == ["elec.apu_gen.1"]


def test_ignoring_the_procedure_fails_the_success_eval():
    """Reading the ECAM and declaring done must not pass: the network is down."""
    script = [
        [("read_ecam", {})],
        [("advance", {"seconds": 5})],
        [("report_done", {"diagnosis": "all good", "actions_summary": "none"})],
    ]
    result, records = _run(script)

    assert result.reason == "agent_done"
    assert result.all_passed is False
    failed_vars = [
        c["var"] for c in records[-1]["success_eval"]["final_state"] if not c["passed"]
    ]
    assert "ELEC_AC_1_BUS_IS_POWERED" in failed_vars


def test_trajectory_is_self_contained_and_ordered():
    """The JSONL carries what #20 needs: meta, gate, ordered calls, sim clock."""
    result, records = _run(PROCEDURE_SCRIPT)

    meta, setup = records[0], records[1]
    assert meta["type"] == "meta"
    assert meta["scenario"]["id"] == "elec-apu-gen-fault"
    assert meta["adapter"]["provider"] == "scripted"
    assert meta["seed"] is None
    assert meta["vendor_pin"] != ""
    # The benchmark surface, recorded: no failure tools, report_done present.
    assert "inject_failure" not in meta["tool_surface"]
    assert "clear_failure" not in meta["tool_surface"]
    assert "report_done" in meta["tool_surface"]

    assert setup["type"] == "setup"
    assert setup["validity_gate"]["passed"] is True
    assert "APU GEN FAULT" in setup["validity_gate"]["ecam"]
    assert setup["active_failures"] == ["elec.apu_gen.1"]

    calls = [r for r in records if r["type"] == "tool_call"]
    assert calls, "tool calls must be recorded"
    times = [c["sim_time_before"] for c in calls] + [calls[-1]["sim_time_after"]]
    assert times == sorted(times), "the simulated clock must be monotonic"
    assert all(not c["is_error"] for c in calls), [c for c in calls if c["is_error"]]


def test_the_agent_cannot_clear_the_failure():
    """clear_failure is not on the benchmark surface; calling it is a recorded error.

    This is the validity property of the whole benchmark: an agent that could
    repair the fault would not be flying the procedure. The error must not
    kill the episode either — it is the agent's problem, not the harness's.
    """
    script = [
        [("clear_failure", {"failure_id": "elec.apu_gen.1"})],
        [("read_ecam", {})],
        [("report_done", {"diagnosis": "cleared it", "actions_summary": "clear_failure"})],
    ]
    result, records = _run(script)

    assert result.reason == "agent_done"
    assert result.all_passed is False, "clearing must not have worked"

    attempted = [r for r in records if r["type"] == "tool_call" and r["name"] == "clear_failure"]
    assert attempted and attempted[0]["is_error"], "the attempt must be recorded as an error"
    assert records[-1]["active_failures"] == ["elec.apu_gen.1"], "the failure must survive"


def test_failed_validity_gate_aborts_as_invalid_scenario():
    """If the promised ECAM never shows, the run is invalid, not scoreable.

    A run where the world did not manifest the failure must never reach the
    agent: scoring it would grade the agent on a scenario that did not happen.
    """
    import yaml

    data = yaml.safe_load(FIRST_SCENARIO.read_text(encoding="utf-8"))
    data["expected_ecam"]["must_appear"] = ["THIS CAUTION DOES NOT EXIST"]
    with tempfile.TemporaryDirectory() as tmp:
        mutated = Path(tmp) / "mutated.yaml"
        mutated.write_text(yaml.safe_dump(data), encoding="utf-8")
        result, records = _run(PROCEDURE_SCRIPT, scenario_path=mutated)

    assert result.reason == "invalid_scenario"
    assert result.valid is False
    assert result.all_passed is None
    assert result.tool_calls_used == 0, "the agent must never have been consulted"
    assert records[1]["validity_gate"]["missing"] == ["THIS CAUTION DOES NOT EXIST"]
    assert records[-1]["success_eval"] is None


def test_tool_call_budget_ends_the_episode():
    scenario = load_scenario(FIRST_SCENARIO)
    endless = [[("read_ecam", {})]] * (scenario.budget.max_tool_calls + 5)
    result, records = _run(endless)

    assert result.reason == "budget_tool_calls"
    assert result.tool_calls_used == scenario.budget.max_tool_calls
    assert records[-1]["success_eval"]["all_passed"] is False


def test_script_exhaustion_is_end_turn_without_done_after_one_nudge():
    result, records = _run([[("read_ecam", {})]])

    assert result.reason == "end_turn_without_done"
    # Exactly one nudge: two trailing assistant records with no tool calls.
    empty_turns = [r for r in records if r["type"] == "assistant" and "script exhausted" in r["text"]]
    assert len(empty_turns) == 2


if __name__ == "__main__":
    tests = sorted(
        (name, fn) for name, fn in globals().items() if name.startswith("test_") and callable(fn)
    )
    for name, fn in tests:
        fn()
        print(f"ok  {name}")
    print(f"\n{len(tests)} episode tests passed.")
