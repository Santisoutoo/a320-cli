"""Scenario loading and validation: YAML in, typed and cross-checked out.

Two validation layers, deliberately separate:

1. **Shape** — jsonschema against ``scenarios/schema/scenario.schema.json``.
   Catches structural mistakes with a JSON-path to the offender.
2. **References** — every failure id, control name and start state is checked
   against the live catalogs (``a320_sim`` + ``a320_mcp.START_STATES``). The
   schema cannot know the catalogs, and a scenario whose failure id does not
   exist would only blow up mid-episode otherwise. Same design as the MCP
   schemas embedding the catalogs as enums (D-017): a name that does not
   exist must fail at load time, loudly.

The loader applies the schema's documented defaults itself (jsonschema
validates defaults, it does not inject them).
"""

import functools
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import jsonschema
import yaml

# bench/a320_bench/scenario.py -> repo root. Scenario data and its schema are
# repo files, not package data: the benchmark runs from a checkout (the vendor
# pin is part of the benchmark's identity), so resolving relative to the repo
# is the honest layout rather than pretending this could ship as a wheel.
REPO_ROOT = Path(__file__).resolve().parents[2]
SCHEMA_PATH = REPO_ROOT / "scenarios" / "schema" / "scenario.schema.json"


class ScenarioError(Exception):
    """A scenario file is invalid — always says which file and which field."""


# --- typed model --------------------------------------------------------------
@dataclass(frozen=True)
class Predicate:
    var: str
    op: str  # eq | ne | gt | ge | lt | le | between
    value: float | None = None
    min: float | None = None
    max: float | None = None


@dataclass(frozen=True)
class Action:
    control: str
    value: float
    rationale: str = ""


@dataclass(frozen=True)
class ForbiddenAction:
    control: str
    value: float
    severity: str  # dangerous | anti_procedure
    rationale: str = ""


@dataclass(frozen=True)
class ProcedureBlock:
    block: str
    actions: tuple[Action, ...]
    ordered: bool = False


@dataclass(frozen=True)
class FailureSpec:
    id: str
    after_setup_s: float | None = None
    when: Predicate | None = None
    settle_s: float = 5.0


@dataclass(frozen=True)
class SourceRef:
    document: str
    revision: str
    accessed: str
    url: str = ""
    notes: str = ""


@dataclass(frozen=True)
class GroundTruth:
    source: SourceRef
    procedure: tuple[ProcedureBlock, ...]
    optional_actions: tuple[Action, ...] = ()
    forbidden_actions: tuple[ForbiddenAction, ...] = ()


@dataclass(frozen=True)
class InitialState:
    start: str
    world_controls: dict[str, float] = field(default_factory=dict)
    set_controls: dict[str, float] = field(default_factory=dict)


@dataclass(frozen=True)
class ExpectedEcam:
    must_appear: tuple[str, ...]
    must_not_appear: tuple[str, ...] = ()


@dataclass(frozen=True)
class Success:
    final_state: tuple[Predicate, ...]
    ecam_clear_of: tuple[str, ...] = ()


@dataclass(frozen=True)
class Budget:
    max_tool_calls: int
    max_sim_time_s: float


@dataclass(frozen=True)
class Scenario:
    id: str
    title: str
    system: str
    initial_state: InitialState
    failures: tuple[FailureSpec, ...]
    expected_ecam: ExpectedEcam
    task_prompt: str
    ground_truth: GroundTruth
    success: Success
    budget: Budget
    instructions_profile: str = "benchmark"
    path: Path | None = None
    raw: dict[str, Any] = field(default_factory=dict, compare=False, repr=False)


# --- predicates ---------------------------------------------------------------
def evaluate_predicate(pred: Predicate, value: float) -> bool:
    """Evaluate one predicate against a read variable value.

    Success criteria are predicates with tolerance windows, never snapshot
    equality: the vendor has real randomness (see the determinism decision in
    docs/decisiones.md), so a scenario asserts *the class* of end state.
    """
    if pred.op == "eq":
        return value == pred.value
    if pred.op == "ne":
        return value != pred.value
    if pred.op == "gt":
        return value > pred.value  # type: ignore[operator]
    if pred.op == "ge":
        return value >= pred.value  # type: ignore[operator]
    if pred.op == "lt":
        return value < pred.value  # type: ignore[operator]
    if pred.op == "le":
        return value <= pred.value  # type: ignore[operator]
    if pred.op == "between":
        return pred.min <= value <= pred.max  # type: ignore[operator]
    raise ScenarioError(f"unknown predicate op '{pred.op}'")  # pragma: no cover - schema-gated


# --- catalogs (cached: building a Sim costs ~1 s) ------------------------------
@functools.lru_cache(maxsize=1)
def _catalog_sim() -> "Any":
    """A throwaway Sim used only as the catalog/validation oracle.

    Never stepped, never observed: `set` may be called on it to let the core
    validate a (control, value) pair — writes land in its variable store but
    the aircraft is never advanced, so nothing accumulates.
    """
    import a320_sim

    return a320_sim.Sim()


@functools.lru_cache(maxsize=1)
def _catalogs() -> tuple[dict[str, str], frozenset[str], frozenset[str], frozenset[str]]:
    """(control name -> domain, failure ids, START_STATES keys, instructions
    profiles), from the live core and the MCP server module."""
    from a320_mcp.server import INSTRUCTIONS_PROFILES, START_STATES

    sim = _catalog_sim()
    domains = {c["name"]: c["domain"] for c in sim.list_controls()}
    failures = frozenset(f["id"] for f in sim.list_failures())
    return domains, failures, frozenset(START_STATES), frozenset(INSTRUCTIONS_PROFILES)


# --- loading -------------------------------------------------------------------
@functools.lru_cache(maxsize=1)
def _schema() -> dict[str, Any]:
    try:
        return json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:  # pragma: no cover - broken checkout
        raise ScenarioError(f"scenario schema not found at {SCHEMA_PATH}") from exc


def _predicate(data: dict[str, Any]) -> Predicate:
    return Predicate(
        var=data["var"],
        op=data["op"],
        value=data.get("value"),
        min=data.get("min"),
        max=data.get("max"),
    )


def _action(data: dict[str, Any]) -> Action:
    return Action(control=data["control"], value=data["value"], rationale=data.get("rationale", ""))


def load_scenario(path: "str | Path", *, check_catalogs: bool = True) -> Scenario:
    """Load and validate one scenario YAML.

    `check_catalogs=False` skips the live cross-checks (control names, failure
    ids, start states) for tests that exercise pure shape validation without
    paying for a Sim.
    """
    path = Path(path)
    try:
        data = yaml.safe_load(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ScenarioError(f"{path}: not found") from exc
    except yaml.YAMLError as exc:
        raise ScenarioError(f"{path}: not valid YAML: {exc}") from exc

    validator = jsonschema.Draft202012Validator(_schema())
    errors = sorted(validator.iter_errors(data), key=lambda e: list(e.absolute_path))
    if errors:
        first = errors[0]
        where = "/".join(str(p) for p in first.absolute_path) or "<root>"
        raise ScenarioError(f"{path}: at {where}: {first.message}")

    scenario = Scenario(
        id=data["id"],
        title=data["title"],
        system=data["system"],
        initial_state=InitialState(
            start=data["initial_state"]["start"],
            world_controls=dict(data["initial_state"].get("world_controls", {})),
            set_controls=dict(data["initial_state"].get("set_controls", {})),
        ),
        failures=tuple(
            FailureSpec(
                id=f["id"],
                after_setup_s=f["at"].get("after_setup_s"),
                when=_predicate(f["at"]["when"]) if "when" in f["at"] else None,
                settle_s=f.get("settle_s", 5.0),
            )
            for f in data["failures"]
        ),
        expected_ecam=ExpectedEcam(
            must_appear=tuple(data["expected_ecam"]["must_appear"]),
            must_not_appear=tuple(data["expected_ecam"].get("must_not_appear", [])),
        ),
        task_prompt=data["task_prompt"],
        ground_truth=GroundTruth(
            source=SourceRef(
                document=data["ground_truth"]["source"]["document"],
                revision=data["ground_truth"]["source"]["revision"],
                accessed=data["ground_truth"]["source"]["accessed"],
                url=data["ground_truth"]["source"].get("url", ""),
                notes=data["ground_truth"]["source"].get("notes", ""),
            ),
            procedure=tuple(
                ProcedureBlock(
                    block=b["block"],
                    ordered=b.get("ordered", False),
                    actions=tuple(_action(a) for a in b["actions"]),
                )
                for b in data["ground_truth"]["procedure"]
            ),
            optional_actions=tuple(
                _action(a) for a in data["ground_truth"].get("optional_actions", [])
            ),
            forbidden_actions=tuple(
                ForbiddenAction(
                    control=a["control"],
                    value=a["value"],
                    severity=a["severity"],
                    rationale=a.get("rationale", ""),
                )
                for a in data["ground_truth"].get("forbidden_actions", [])
            ),
        ),
        success=Success(
            final_state=tuple(_predicate(p) for p in data["success"]["final_state"]),
            ecam_clear_of=tuple(data["success"].get("ecam_clear_of", [])),
        ),
        budget=Budget(
            max_tool_calls=data["budget"]["max_tool_calls"],
            max_sim_time_s=data["budget"]["max_sim_time_s"],
        ),
        instructions_profile=data.get("instructions_profile", "benchmark"),
        path=path,
        raw=data,
    )

    # Every predicate in the file, not only success criteria: a failure trigger
    # with an empty window would otherwise hang the injection wait mid-episode.
    predicates = [("success", pred) for pred in scenario.success.final_state]
    predicates += [
        ("failure trigger", failure.when) for failure in scenario.failures if failure.when
    ]
    for where, pred in predicates:
        if pred.op == "between" and pred.min > pred.max:  # type: ignore[operator]
            raise ScenarioError(
                f"{path}: {where} predicate on '{pred.var}': min {pred.min} > max {pred.max}"
            )

    if check_catalogs:
        _cross_check(scenario, path)
    return scenario


def _cross_check(scenario: Scenario, path: Path) -> None:
    """Every reference must exist in the live catalogs — fail at load, not mid-episode."""
    domains, failure_ids, start_states, instructions_profiles = _catalogs()

    if scenario.initial_state.start not in start_states:
        raise ScenarioError(
            f"{path}: unknown start state '{scenario.initial_state.start}' "
            f"(expected one of {sorted(start_states)})"
        )

    if scenario.instructions_profile not in instructions_profiles:
        raise ScenarioError(
            f"{path}: unknown instructions_profile '{scenario.instructions_profile}' "
            f"(expected one of {sorted(instructions_profiles)}; see "
            f"a320_mcp.server.INSTRUCTIONS_PROFILES)"
        )

    for failure in scenario.failures:
        if failure.id not in failure_ids:
            raise ScenarioError(
                f"{path}: unknown failure id '{failure.id}' (not in the core catalog; "
                f"see list_failures())"
            )

    def check_control(
        name: str,
        where: str,
        *,
        must_be_world: bool = False,
        cockpit_only: bool = False,
        value: "float | None" = None,
    ) -> None:
        if name not in domains:
            raise ScenarioError(
                f"{path}: unknown control '{name}' in {where} (not in the core catalog; "
                f"see list_controls())"
            )
        if must_be_world and domains[name] != "world":
            raise ScenarioError(
                f"{path}: control '{name}' in {where} has domain '{domains[name]}', "
                f"but world_controls may only pre-set domain=world controls — cockpit "
                f"state belongs in set_controls or in the agent's hands"
            )
        if cockpit_only and domains[name] == "world":
            raise ScenarioError(
                f"{path}: control '{name}' in {where} has domain 'world' — world "
                f"state belongs in world_controls, not among cockpit overrides"
            )
        if value is not None:
            # The core is the oracle for valid values (same philosophy as
            # D-017): its catalog strings ('0 (off) or 1 (on)', 'one of
            # [...]') are for humans, `set` is the machine-checkable truth.
            # The catalog Sim is never advanced, so the write is inert.
            import a320_sim

            try:
                _catalog_sim().set(name, value)
            except a320_sim.SimError as exc:
                raise ScenarioError(f"{path}: in {where}: {exc}") from exc

    for name, value in scenario.initial_state.world_controls.items():
        check_control(name, "initial_state.world_controls", must_be_world=True, value=value)
    for name, value in scenario.initial_state.set_controls.items():
        check_control(name, "initial_state.set_controls", cockpit_only=True, value=value)
    for block in scenario.ground_truth.procedure:
        for action in block.actions:
            check_control(
                action.control, f"procedure block '{block.block}'", value=action.value
            )
    for action in scenario.ground_truth.optional_actions:
        check_control(action.control, "optional_actions", value=action.value)
    for forbidden in scenario.ground_truth.forbidden_actions:
        check_control(forbidden.control, "forbidden_actions", value=forbidden.value)
