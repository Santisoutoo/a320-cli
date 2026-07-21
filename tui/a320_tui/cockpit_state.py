"""In-memory state for every cockpit control the sim does not (yet) back.

The requirement is that every control of the vendored model is built and
interactable even before it has sim functionality: pushing a korry, opening
a guard, cycling a selector or moving a lever must work and be reflected,
just without touching the aircraft. ``CockpitRegistry`` owns that local
state; it knows nothing about Textual or ``a320_sim`` (pure, testable).

Semantics per type follow the YAML ``types:`` legend: ``pb`` latches,
``pb_mom`` is spring-loaded, ``pb_guard`` needs its red guard opened first,
``fire_pb`` pops out, ``sel``/``sw``/``lever`` move across positions with
end stops, ``knob``/``wheel`` are continuous with clamping, ``knob_pp``
adds push (managed) / pull (selected). ``light``/``gauge`` are indications
only. Airbus lights-out: everything starts dark and released (cold & dark)
unless the model says otherwise (``def`` / ``def_on``).
"""

from __future__ import annotations

from dataclasses import dataclass, field

from a320_tui.model import CockpitModel, ControlDef

_POSITIONAL = ("sel", "sw", "lever")
_PUSHABLE = ("pb", "pb_mom", "pb_guard", "fire_pb")


@dataclass
class ControlState:
    """Union of the per-type state; only the fields for its type are used."""

    pressed: bool = False
    guard_open: bool = False
    popped_out: bool = False
    pos: str | None = None
    value: float | None = None
    managed: bool = False
    lights_on: frozenset[str] = field(default_factory=frozenset)


@dataclass(frozen=True)
class ActionResult:
    """Outcome of one actuation, worded for the command log."""

    message: str
    changed: bool


class CockpitRegistry:
    """id -> ControlState for the whole model, initialized cold & dark."""

    def __init__(self, model: CockpitModel) -> None:
        self._defs = model.by_id
        self.states: dict[str, ControlState] = {
            cdef.id: _initial_state(cdef) for cdef in model.controls
        }

    def definition(self, control_id: str) -> ControlDef:
        return self._defs[control_id]

    def state(self, control_id: str) -> ControlState:
        return self.states[control_id]

    # -- actions ----------------------------------------------------------

    def press(self, control_id: str) -> ActionResult:
        cdef, state = self._defs[control_id], self.states[control_id]
        if cdef.ctype == "pb":
            state.pressed = not state.pressed
            return ActionResult(
                "pressed" if state.pressed else "released", True
            )
        if cdef.ctype == "pb_mom":
            # Spring-loaded: acts, never latches.
            return ActionResult("pressed (momentary)", True)
        if cdef.ctype == "pb_guard":
            if not state.guard_open:
                return ActionResult("guarded — open the guard first", False)
            state.pressed = not state.pressed
            return ActionResult(
                "pressed" if state.pressed else "released", True
            )
        if cdef.ctype == "fire_pb":
            state.popped_out = not state.popped_out
            return ActionResult(
                "popped out" if state.popped_out else "stowed", True
            )
        return ActionResult(f"{cdef.ctype} is not pushable", False)

    def open_guard(self, control_id: str) -> ActionResult:
        cdef, state = self._defs[control_id], self.states[control_id]
        if cdef.ctype != "pb_guard":
            return ActionResult("no guard on this control", False)
        if state.guard_open:
            return ActionResult("guard already open", False)
        state.guard_open = True
        return ActionResult("guard open", True)

    def close_guard(self, control_id: str) -> ActionResult:
        cdef, state = self._defs[control_id], self.states[control_id]
        if cdef.ctype != "pb_guard" or not state.guard_open:
            return ActionResult("guard already closed", False)
        state.guard_open = False
        return ActionResult("guard closed", True)

    def cycle(self, control_id: str, step: int) -> ActionResult:
        """Move a positional control one notch; end stops, no wrap."""
        cdef, state = self._defs[control_id], self.states[control_id]
        positions = cdef.positions
        if cdef.ctype not in _POSITIONAL and not (
            cdef.ctype == "knob" and positions
        ):
            return ActionResult(f"{cdef.ctype} has no positions", False)
        if not positions:
            return ActionResult("no positions modeled", False)
        current = positions.index(state.pos) if state.pos in positions else 0
        target = min(max(current + step, 0), len(positions) - 1)
        if target == current and state.pos in positions:
            return ActionResult(f"already at {positions[target]}", False)
        state.pos = positions[target]
        return ActionResult(f"-> {state.pos}", True)

    def move(self, control_id: str, pos: str) -> ActionResult:
        cdef, state = self._defs[control_id], self.states[control_id]
        if pos not in cdef.positions:
            return ActionResult(
                f"unknown position {pos!r} (valid: {', '.join(cdef.positions)})",
                False,
            )
        if state.pos == pos:
            return ActionResult(f"already at {pos}", False)
        state.pos = pos
        return ActionResult(f"-> {pos}", True)

    def delta(self, control_id: str, amount: float) -> ActionResult:
        """Turn a continuous control, clamped to its modeled range."""
        cdef, state = self._defs[control_id], self.states[control_id]
        if cdef.ctype not in ("knob", "knob_pp", "wheel"):
            return ActionResult(f"{cdef.ctype} is not continuous", False)
        low, high = cdef.value_range or _default_range(cdef)
        current = state.value if state.value is not None else low
        target = min(max(current + amount, low), high)
        state.pos = None  # leaving a special detent (LDG ELEV AUTO)
        if target == current and state.value is not None:
            return ActionResult(f"at range end ({target:g})", False)
        state.value = target
        return ActionResult(f"-> {target:g}", True)

    def push(self, control_id: str) -> ActionResult:
        cdef, state = self._defs[control_id], self.states[control_id]
        if cdef.ctype != "knob_pp":
            return ActionResult("push/pull only on FCU-style knobs", False)
        state.managed = True
        return ActionResult("pushed (managed)", True)

    def pull(self, control_id: str) -> ActionResult:
        cdef, state = self._defs[control_id], self.states[control_id]
        if cdef.ctype != "knob_pp":
            return ActionResult("push/pull only on FCU-style knobs", False)
        state.managed = False
        return ActionResult("pulled (selected)", True)


def _default_range(cdef: ControlDef) -> tuple[float, float]:
    # Wheels swing both ways around neutral; plain knobs are 0..1 volume-style.
    if cdef.ctype == "wheel":
        return (-1.0, 1.0)
    return (0.0, 1.0)


def _initial_state(cdef: ControlDef) -> ControlState:
    state = ControlState()
    if cdef.ctype in _PUSHABLE:
        state.pressed = cdef.def_on
    elif cdef.ctype in _POSITIONAL:
        if isinstance(cdef.default, str):
            state.pos = cdef.default
        elif cdef.positions:
            state.pos = cdef.positions[0]
    elif cdef.ctype in ("knob", "knob_pp"):
        if isinstance(cdef.default, str):
            state.pos = cdef.default  # special detent (LDG ELEV AUTO)
        elif cdef.default is not None:
            state.value = float(cdef.default)
        elif cdef.positions:
            state.pos = cdef.positions[0]
        else:
            state.value = (cdef.value_range or _default_range(cdef))[0]
    elif cdef.ctype in ("wheel", "gauge"):
        state.value = 0.0
    return state
