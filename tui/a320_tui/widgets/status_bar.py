"""StatusBar: sim time, time mode and failure count in one docked line."""

from __future__ import annotations

from rich.text import Text
from textual.widgets import Static

from a320_tui.state import SimState


class StatusBar(Static):
    DEFAULT_CSS = """
    StatusBar {
        dock: top;
        height: 1;
        background: $panel;
        color: $text;
        padding: 0 1;
    }
    """

    def update_status(self, state: SimState, paused: bool, speed: int) -> None:
        mode = "⏸ paused" if paused else f"▶ x{speed}"
        text = Text()
        text.append("A320 systems twin", "bold")
        text.append(f"   t={state.t:8.2f}s   {mode}")
        n = len(state.active_failures)
        if n:
            text.append(f"   {n} failure(s) active", "orange3")
        else:
            text.append("   no failures", "grey50")
        self.update(text)
