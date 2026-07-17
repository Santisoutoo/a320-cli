"""SimBridge: the single owner of the ``a320_sim.Sim`` instance.

``Sim`` is *unsendable* (PyO3: bound to the thread that created it, because
FBW's systems use ``Rc``/``RefCell`` internally). Everything in the TUI that
touches the sim goes through this bridge, and the bridge asserts it is always
called from the thread that created it. Textual timers (``set_interval``) run
on the event-loop thread, so the rule in practice is: never hand the bridge to
a thread worker (``@work(thread=True)`` is forbidden for sim access).
"""

from __future__ import annotations

import threading

import a320_sim

from a320_tui.manifest import manifest_vars
from a320_tui.state import SimState


class SimBridge:
    def __init__(self, sim: "a320_sim.Sim | None" = None) -> None:
        self.sim = sim if sim is not None else a320_sim.Sim()
        self._thread = threading.get_ident()
        self.controls = self.sim.list_controls()
        # Failure support arrived with Phase 2; degrade gracefully on an older
        # a320_sim build. The catalog maps id -> description for the E/WD.
        self.supports_failures = hasattr(self.sim, "inject_failure")
        self.failure_catalog: dict[str, str] = {}
        if self.supports_failures:
            self.failure_catalog = {
                f["id"]: f["description"] for f in self.sim.list_failures()
            }
        # Selective read set for the tick. Vars missing from the registry (a
        # vendor update could rename one) read as 0.0 instead of failing the
        # whole refresh; tests/test_manifest.py flags them loudly.
        wanted = manifest_vars(self.controls)
        known = set(self.sim.list_variables())
        self._vars = [v for v in wanted if v in known]
        self.missing_vars = [v for v in wanted if v not in known]

    def _check_thread(self) -> None:
        assert threading.get_ident() == self._thread, (
            "SimBridge used from a different thread: a320_sim.Sim is "
            "unsendable; keep all sim access on the main event-loop thread"
        )

    def step(self, dt_ms: int) -> None:
        self._check_thread()
        self.sim.step(dt_ms)

    def set(self, control: str, value: float) -> None:
        """Actuate a control. Raises ``a320_sim.SimError`` on bad input."""
        self._check_thread()
        self.sim.set(control, value)

    def read_state(self) -> SimState:
        self._check_thread()
        active = ()
        if self.supports_failures:
            active = tuple(sorted(self.sim.active_failures()))
        return SimState(
            t=self.sim.sim_time(),
            vars=self.sim.get(self._vars) if self._vars else {},
            active_failures=active,
        )
