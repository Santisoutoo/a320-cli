"""CockpitController: routes actuations and builds the render views.

One decision lives here: a control whose id is in the wiring map belongs to
the sim (its widget renders from ``SimState`` and its actuation goes through
``SimBridge.set``); everything else operates on the local
``CockpitRegistry``. Widgets never decide this themselves — they post the
same ``actuate`` intent regardless.
"""

from __future__ import annotations

from collections.abc import Callable, Mapping
from dataclasses import dataclass
from typing import Any

from a320_tui.cockpit_state import ActionResult, CockpitRegistry
from a320_tui.model import CockpitModel


@dataclass(frozen=True)
class ControlView:
    """Everything a widget needs to render one local control."""

    id: str
    legend: str
    ctype: str
    lights: tuple[str, ...] = ()
    lit: frozenset[str] = frozenset()
    positions: tuple[str, ...] = ()
    pos: str | None = None
    value: float | None = None
    value_range: tuple[float, float] | None = None
    pressed: bool = False
    guard_open: bool = False
    popped_out: bool = False
    managed: bool = False
    keys: tuple[str, ...] = ()
    wired: bool = False


class CockpitController:
    """Owns the actuation routing for the whole cockpit."""

    def __init__(
        self,
        model: CockpitModel,
        registry: CockpitRegistry,
        wired_ids: frozenset[str] = frozenset(),
        set_wired: Callable[[str, Any], None] | None = None,
    ) -> None:
        self.model = model
        self.registry = registry
        self.wired_ids = wired_ids
        self._set_wired = set_wired

    def is_wired(self, control_id: str) -> bool:
        return control_id in self.wired_ids

    def actuate(
        self, control_id: str, action: str, payload: Any = None
    ) -> ActionResult:
        """Apply one actuation; the message is worded for the command log."""
        if control_id not in self.model.by_id:
            return ActionResult(f"unknown control {control_id!r}", False)
        if self.is_wired(control_id):
            if self._set_wired is None:
                return ActionResult("wired control without a sim bridge", False)
            self._set_wired(control_id, payload)
            return ActionResult("-> sim", True)
        registry = self.registry
        if action == "press":
            return registry.press(control_id)
        if action == "open_guard":
            return registry.open_guard(control_id)
        if action == "close_guard":
            return registry.close_guard(control_id)
        if action == "cycle":
            return registry.cycle(control_id, int(payload))
        if action == "move":
            return registry.move(control_id, str(payload))
        if action == "delta":
            return registry.delta(control_id, float(payload))
        if action == "push":
            return registry.push(control_id)
        if action == "pull":
            return registry.pull(control_id)
        return ActionResult(f"unknown action {action!r}", False)

    def view(self, control_id: str) -> ControlView:
        """Render view of a local control (wired ones render from SimState)."""
        cdef = self.model.by_id[control_id]
        state = self.registry.state(control_id)
        return ControlView(
            id=cdef.id,
            legend=_legend(cdef.base_id),
            ctype=cdef.ctype,
            lights=cdef.lights,
            lit=state.lights_on,
            positions=cdef.positions,
            pos=state.pos,
            value=state.value,
            value_range=cdef.value_range,
            pressed=state.pressed,
            guard_open=state.guard_open,
            popped_out=state.popped_out,
            managed=state.managed,
            keys=cdef.keys,
            wired=self.is_wired(cdef.id),
        )


def _legend(base_id: str) -> str:
    return base_id.replace("_", " ")
