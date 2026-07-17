"""EwdPanel: the warnings area of the E/WD.

Until Phase 2's ``read_ecam`` (#15) lands, this is an honest placeholder: it
lists the *injected* failures (ground truth, not the FWC's detection) as amber
caution-style lines built from the failure catalog. Once the installed
``a320_sim`` grows ``read_ecam``, ``SimBridge`` can feed structured warnings
and only the line-building here changes.
"""

from __future__ import annotations

from rich.text import Text
from textual.widgets import Static

from a320_tui.state import SimState

_AMBER = "orange3"
_GREEN = "green3"
_DIM = "grey50"


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
        if not state.active_failures:
            text.append("no failures active\n", _GREEN)
        else:
            for fid in state.active_failures:
                desc = self._catalog.get(fid, fid)
                text.append(f"{desc}\n", _AMBER)
                text.append(f"  ({fid})\n", _DIM)
        text.append("raw injected-failure view — FWC detection (read_ecam, #15) pending", _DIM)
        self.update(text)
