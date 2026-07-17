"""EwdPanel: the warnings area of the E/WD, fed by ``read_ecam``.

Two layers, deliberately distinct:

- **ECAM lines** (top): what the detection layer reports — red warnings, amber
  cautions, severity-ordered by the core. This is what a pilot (or the MCP
  agent) sees. An empty ECAM on an unpowered aircraft is also faithful: no
  power, no display (the core's power gate, D-014).
- **Injected ground truth** (bottom, dim): which failures the *scenario* has
  active. A pilot never sees this — it exists because the TUI is the harness
  operator's cockpit, and the operator is the one injecting. The visual split
  raw-vs-detected is the whole Phase 2 story on one screen.

On an ``a320_sim`` built before Phase 2 (no ``read_ecam``), degrades to the
ground-truth list alone.
"""

from __future__ import annotations

from rich.text import Text
from textual.widgets import Static

from a320_tui.state import SimState

_RED = "bold red3"
_AMBER = "orange3"
_GREEN = "green3"
_DIM = "grey50"

_SEVERITY_STYLE = {"warning": _RED, "caution": _AMBER, "advisory": _GREEN}


class EwdPanel(Static):
    DEFAULT_CSS = """
    EwdPanel {
        border: round $primary-darken-2;
        padding: 0 1;
        height: 9;
        overflow-y: auto;
    }
    """

    def __init__(self, failure_catalog: dict[str, str]) -> None:
        super().__init__()
        self.border_title = "E/WD"
        self._catalog = failure_catalog

    def update_state(self, state: SimState) -> None:
        text = Text()
        if state.ecam:
            for severity, message, source in state.ecam:
                style = _SEVERITY_STYLE.get(severity, _AMBER)
                text.append(f"{message}\n", style)
                if source != "vendor_flag":
                    # Our rule, not FBW's flag — the honesty marker of D-014.
                    text.append("  (derived)\n", _DIM)
        else:
            text.append("ECAM clear\n", _GREEN)

        if state.active_failures:
            text.append("\n")
            text.append("injected (scenario ground truth):\n", _DIM)
            for fid in state.active_failures:
                text.append(f"  {fid} — {self._catalog.get(fid, fid)}\n", _DIM)
        self.update(text)
