"""Scenario loading and validation tests (#69).

Two layers under test: shape (jsonschema, no Sim needed) and references
(live catalog cross-checks). Invalid scenarios must fail at load time with a
message that names the file and the offending field — a scenario that only
blows up mid-episode wastes an LLM run.

Runnable two ways, same as the MCP tests:
  - directly:     python bench/tests/test_scenario_schema.py
  - under pytest: pytest bench/tests/
"""

import copy
import tempfile
from pathlib import Path

import yaml

from a320_bench import Scenario, ScenarioError, evaluate_predicate, load_scenario
from a320_bench.scenario import Predicate, REPO_ROOT

FIRST_SCENARIO = REPO_ROOT / "scenarios" / "elec" / "apu_gen_fault.yaml"


def _base() -> dict:
    return yaml.safe_load(FIRST_SCENARIO.read_text(encoding="utf-8"))


def _load_mutated(data: dict, *, check_catalogs: bool = True) -> Scenario:
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "scenario.yaml"
        path.write_text(yaml.safe_dump(data), encoding="utf-8")
        return load_scenario(path, check_catalogs=check_catalogs)


def _expect_error(data: dict, *needles: str, check_catalogs: bool = True) -> None:
    try:
        _load_mutated(data, check_catalogs=check_catalogs)
    except ScenarioError as exc:
        for needle in needles:
            assert needle in str(exc), f"error should mention '{needle}': {exc}"
    else:
        raise AssertionError(f"expected ScenarioError mentioning {needles}")


# --- the real scenario -------------------------------------------------------
def test_first_scenario_loads_with_catalog_cross_checks():
    """The shipped APU GEN scenario is valid against schema AND catalogs.

    This is the test that keeps the dataset honest: if the core renames a
    control or a failure id, this fails at CI time instead of mid-benchmark.
    """
    scenario = load_scenario(FIRST_SCENARIO)

    assert scenario.id == "elec-apu-gen-fault"
    assert scenario.system == "ELEC"
    assert scenario.initial_state.start == "apu-running"
    assert scenario.initial_state.world_controls == {"ext_pwr_avail": 1}
    assert [f.id for f in scenario.failures] == ["elec.apu_gen.1"]
    assert scenario.failures[0].after_setup_s == 10
    assert scenario.failures[0].settle_s == 5
    assert "APU GEN FAULT" in scenario.expected_ecam.must_appear

    blocks = scenario.ground_truth.procedure
    assert [b.block for b in blocks] == ["reset_attempt", "restore_power"]
    assert blocks[0].ordered and not blocks[1].ordered
    assert blocks[0].actions[0].control == "apu_gen"

    assert scenario.ground_truth.source.url, "the citation must carry a verifiable URL"
    assert scenario.ground_truth.source.accessed == "2026-07-23"
    assert scenario.instructions_profile == "benchmark"
    assert scenario.budget.max_tool_calls == 40


def test_task_prompt_does_not_leak_the_ground_truth():
    """The prompt must not hand the agent the diagnosis (D-016 spirit).

    A prompt that names the failed system or the failure id turns diagnosis
    into reading comprehension. Checked for every scenario in the suite.
    """
    for path in sorted((REPO_ROOT / "scenarios").rglob("*.yaml")):
        scenario = load_scenario(path, check_catalogs=False)
        prompt = scenario.task_prompt.lower()
        for failure in scenario.failures:
            for token in failure.id.split("."):
                if len(token) > 3:  # 'elec'/'apu_gen' yes; bus indices no
                    assert token not in prompt, (
                        f"{path.name}: task_prompt leaks '{token}' from {failure.id}"
                    )
        for message in scenario.expected_ecam.must_appear:
            assert message.lower() not in prompt, (
                f"{path.name}: task_prompt leaks the expected ECAM '{message}'"
            )


# --- shape errors (no Sim needed) ---------------------------------------------
def test_missing_required_section_names_the_field():
    data = _base()
    del data["ground_truth"]
    _expect_error(data, "ground_truth", check_catalogs=False)


def test_between_predicate_requires_min_and_max():
    data = _base()
    data["success"]["final_state"] = [{"var": "X", "op": "between", "value": 1}]
    _expect_error(data, "final_state", check_catalogs=False)


def test_inverted_between_bounds_are_rejected():
    data = _base()
    data["success"]["final_state"] = [{"var": "X", "op": "between", "min": 5, "max": 1}]
    _expect_error(data, "min 5", "max 1", check_catalogs=False)


def test_inverted_between_bounds_in_failure_trigger_are_rejected():
    """The empty-window check covers `failures[].at.when` too, not only success.

    A trigger predicate that can never hold would hang the injection wait
    mid-episode — exactly the class of error load time exists to catch.
    """
    data = _base()
    data["failures"][0]["at"] = {"when": {"var": "X", "op": "between", "min": 5, "max": 1}}
    _expect_error(data, "min 5", "max 1", check_catalogs=False)


def test_unknown_top_level_key_is_rejected():
    """additionalProperties: false — a typo'd section must not pass silently."""
    data = _base()
    data["succes"] = data["success"]
    _expect_error(data, "succes", check_catalogs=False)


# --- reference errors (live catalogs) -------------------------------------------
def test_unknown_failure_id_is_rejected():
    data = _base()
    data["failures"][0]["id"] = "elec.flux_capacitor.1"
    _expect_error(data, "elec.flux_capacitor.1", "catalog")


def test_unknown_control_in_procedure_is_rejected():
    data = _base()
    data["ground_truth"]["procedure"][0]["actions"][0]["control"] = "no_such_pb"
    _expect_error(data, "no_such_pb", "reset_attempt")


def test_world_controls_must_be_world_domain():
    """A cockpit control in world_controls is a scenario-design error.

    World state is the scenario's to fix; cockpit state is the agent's to
    manage (mcp/README.md, Phase 3 closure note). bat_1 is a cockpit pb.
    """
    data = _base()
    data["initial_state"]["world_controls"]["bat_1"] = 1
    _expect_error(data, "bat_1", "world")


def test_unknown_start_state_is_rejected():
    data = _base()
    data["initial_state"]["start"] = "hangar"
    _expect_error(data, "hangar")


# --- predicates ------------------------------------------------------------------
def test_predicate_evaluation():
    assert evaluate_predicate(Predicate(var="v", op="eq", value=1.0), 1.0)
    assert not evaluate_predicate(Predicate(var="v", op="eq", value=1.0), 0.0)
    assert evaluate_predicate(Predicate(var="v", op="between", min=2800, max=3100), 2950.0)
    assert not evaluate_predicate(Predicate(var="v", op="between", min=2800, max=3100), 100.0)
    assert evaluate_predicate(Predicate(var="v", op="ge", value=0.5), 0.5)
    assert not evaluate_predicate(Predicate(var="v", op="lt", value=0.5), 0.5)


if __name__ == "__main__":
    tests = sorted(
        (name, fn) for name, fn in globals().items() if name.startswith("test_") and callable(fn)
    )
    for name, fn in tests:
        fn()
        print(f"ok  {name}")
    print(f"\n{len(tests)} scenario tests passed.")
