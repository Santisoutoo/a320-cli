"""ToggleSwitch: a 2-3 position switch, `<X>` in the mockup notation."""

from __future__ import annotations

from rich.text import Text

from a320_tui.controller import ControlView
from a320_tui.widgets.base import DARK, WHITE, CockpitControlWidget


def render_switch(view: ControlView) -> Text:
    """One line per position, the live one marked: ``▸ AUTO``."""
    text = Text(justify="left")
    if not view.positions:
        text.append("<no positions>", DARK)
        return text
    for i, pos in enumerate(view.positions):
        if i:
            text.append("\n")
        if pos == view.pos:
            text.append(f"▸ {pos}", WHITE)
        else:
            text.append(f"  {pos}", DARK)
    return text


class ToggleSwitch(CockpitControlWidget):
    def __init__(self, view: ControlView) -> None:
        super().__init__(view)
        self.styles.height = max(3, len(view.positions) + 2)
        width = max(len(p) for p in view.positions) + 8 if view.positions else 14
        self.styles.width = max(width, len(view.legend) + 6)

    def render_view(self, view: ControlView) -> Text:
        return render_switch(view)

    def key_enter(self) -> None:
        self._act("cycle", 1)

    def key_right_square_bracket(self) -> None:
        self._act("cycle", 1)

    def key_left_square_bracket(self) -> None:
        self._act("cycle", -1)
