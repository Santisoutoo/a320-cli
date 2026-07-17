"""SimState: an immutable snapshot of everything the widgets render.

Widgets never touch ``a320_sim.Sim`` directly (it is unsendable and owned by
``SimBridge``); they receive a ``SimState`` and render from it. Being a frozen
dataclass with value equality, the app can skip re-rendering when nothing
changed between ticks.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Mapping, Tuple


@dataclass(frozen=True)
class SimState:
    t: float
    vars: Mapping[str, float] = field(default_factory=dict)
    active_failures: Tuple[str, ...] = ()
    # ECAM lines from read_ecam(), already severity-ordered by the core:
    # (severity, message, source) — e.g. ("caution", "APU GEN FAULT", "vendor_flag").
    ecam: Tuple[Tuple[str, str, str], ...] = ()

    def value(self, name: str) -> float:
        return self.vars.get(name, 0.0)

    def is_on(self, name: str) -> bool:
        return self.value(name) != 0.0
