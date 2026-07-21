"""KeyGroup: one focusable widget for a whole key cluster (MCDU keyboard,
ACP knobs, transponder keypad...). The model keeps these as a single
composite ControlDef (a MCDU keyboard expanded would be ~50 widgets with
no interactive value today); the widget navigates its keys internally with
``[`` / ``]`` and presses the highlighted one with Enter."""

from __future__ import annotations

from rich.text import Text

from a320_tui.controller import ControlView
from a320_tui.widgets.base import DARK, WHITE, CockpitControlWidget

_PER_ROW = 6


def render_key_group(view: ControlView, cursor: int) -> Text:
    text = Text(justify="center")
    for i, key in enumerate(view.keys):
        if i and i % _PER_ROW == 0:
            text.append("\n")
        style = WHITE if i == cursor else DARK
        text.append(f"[{key}]", style)
    if not view.keys:
        text.append("<empty group>", DARK)
    return text


class KeyGroup(CockpitControlWidget):
    def __init__(self, view: ControlView) -> None:
        super().__init__(view)
        self._cursor = 0
        rows = max(1, -(-len(view.keys) // _PER_ROW))
        self.styles.height = rows + 2
        self.styles.width = "auto"

    def render_view(self, view: ControlView) -> Text:
        return render_key_group(view, self._cursor)

    def _move_cursor(self, step: int) -> None:
        if self._view.keys:
            self._cursor = (self._cursor + step) % len(self._view.keys)
            self.refresh_view(self._view)

    def key_right_square_bracket(self) -> None:
        self._move_cursor(1)

    def key_left_square_bracket(self) -> None:
        self._move_cursor(-1)

    def key_enter(self) -> None:
        if self._view.keys:
            self._act("press", self._view.keys[self._cursor])
