"""Rotary controls: selectors, continuous knobs and FCU push/pull knobs."""

from __future__ import annotations

from rich.text import Text

from a320_tui.controller import ControlView
from a320_tui.widgets.base import (
    BLUE,
    DARK,
    WHITE,
    CockpitControlWidget,
)

_BAR_CELLS = 10


def render_selector(view: ControlView) -> Text:
    """``(OFF·NAV·ATT)`` with the live position highlighted."""
    text = Text(justify="center")
    text.append("(", DARK)
    for i, pos in enumerate(view.positions):
        if i:
            text.append("·", DARK)
        text.append(pos, WHITE if pos == view.pos else DARK)
    text.append(")", DARK)
    return text


def render_knob(view: ControlView) -> Text:
    """Value readout over a fill bar; special detents render as their label."""
    text = Text(justify="center")
    if view.pos is not None:
        # Special detent (LDG ELEV AUTO) or categorical end label.
        text.append(f"({view.pos})", WHITE)
        return text
    low, high = view.value_range or (0.0, 1.0)
    value = view.value if view.value is not None else low
    text.append(f"{value:g}", WHITE)
    text.append("\n")
    span = high - low
    filled = 0 if span <= 0 else round((value - low) / span * _BAR_CELLS)
    text.append("█" * filled, BLUE)
    text.append("░" * (_BAR_CELLS - filled), DARK)
    return text


def render_knob_pp(view: ControlView) -> Text:
    """FCU knob: value plus the managed dot (push=managed, pull=selected)."""
    text = render_knob(view)
    text.append("\n")
    if view.managed:
        text.append("● managed", BLUE)
    else:
        text.append("○ selected", WHITE)
    return text


class RotarySelector(CockpitControlWidget):
    """``sel``, and ``knob`` re-typed categorical (COLD..HOT)."""

    def __init__(self, view: ControlView) -> None:
        super().__init__(view)
        self.styles.height = 3
        width = sum(len(p) + 1 for p in view.positions) + 5
        self.styles.width = max(width, len(view.legend) + 6)

    def render_view(self, view: ControlView) -> Text:
        return render_selector(view)

    def key_enter(self) -> None:
        self._act("cycle", 1)

    def key_right_square_bracket(self) -> None:
        self._act("cycle", 1)

    def key_left_square_bracket(self) -> None:
        self._act("cycle", -1)


class Knob(CockpitControlWidget):
    """Continuous ``knob``; step = 1/20th of the modeled range."""

    def __init__(self, view: ControlView) -> None:
        super().__init__(view)
        self.styles.height = 4

    def _step(self) -> float:
        low, high = self._view.value_range or (0.0, 1.0)
        return (high - low) / 20 or 0.05

    def render_view(self, view: ControlView) -> Text:
        return render_knob(view)

    def key_enter(self) -> None:
        self._act("delta", self._step())

    def key_right_square_bracket(self) -> None:
        self._act("delta", self._step())

    def key_left_square_bracket(self) -> None:
        self._act("delta", -self._step())


class KnobPushPull(Knob):
    """FCU-style: Enter = push (managed), p = pull (selected)."""

    def __init__(self, view: ControlView) -> None:
        super().__init__(view)
        self.styles.height = 5

    def render_view(self, view: ControlView) -> Text:
        return render_knob_pp(view)

    def key_enter(self) -> None:
        self._act("push")

    def key_p(self) -> None:
        self._act("pull")
