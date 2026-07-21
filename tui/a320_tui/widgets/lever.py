"""Lever with detents (thrust, flaps, speed brake, gear, park brake)."""

from __future__ import annotations

from rich.text import Text

from a320_tui.controller import ControlView
from a320_tui.widgets.base import DARK, WHITE, CockpitControlWidget


def render_lever(view: ControlView) -> Text:
    """Detent ladder, last position of the list on top (TOGA over REV)."""
    text = Text(justify="left")
    for i, pos in enumerate(reversed(view.positions)):
        if i:
            text.append("\n")
        if pos == view.pos:
            text.append(f"█ {pos}", WHITE)
        else:
            text.append(f"│ {pos}", DARK)
    return text


class Lever(CockpitControlWidget):
    def __init__(self, view: ControlView) -> None:
        super().__init__(view)
        self.styles.height = max(3, len(view.positions) + 2)
        width = max(len(p) for p in view.positions) + 8 if view.positions else 14
        self.styles.width = max(width, len(view.legend) + 6)

    def render_view(self, view: ControlView) -> Text:
        return render_lever(view)

    def key_enter(self) -> None:
        self._act("cycle", 1)

    def key_right_square_bracket(self) -> None:
        self._act("cycle", 1)

    def key_left_square_bracket(self) -> None:
        self._act("cycle", -1)
