"""Indications: annunciator lights and gauge placeholders (not actuable)."""

from __future__ import annotations

from rich.text import Text
from textual.widgets import Static

from a320_tui.controller import ControlView
from a320_tui.widgets.base import AMBER, DARK, WHITE


def render_light(view: ControlView) -> Text:
    text = Text(justify="center")
    labels = view.lights or (view.legend,)
    for i, label in enumerate(labels):
        if i:
            text.append(" ")
        text.append(label, AMBER if label in view.lit else DARK)
    return text


def render_gauge(view: ControlView) -> Text:
    text = Text(justify="center")
    if view.value not in (None, 0.0):
        text.append(f"{view.value:g}", WHITE)
    else:
        text.append("▁▂▁▂▁", DARK)
    return text


class LightAnnunciator(Static):
    """``light``: display only, never focusable."""

    DEFAULT_CSS = """
    LightAnnunciator {
        width: auto;
        height: 3;
        border: round $primary-darken-3;
        content-align: center middle;
        text-align: center;
        color: $text-muted;
    }
    """

    def __init__(self, view: ControlView) -> None:
        super().__init__()
        self._view = view
        self.border_title = view.legend
        self.styles.width = max(12, len(view.legend) + 6)

    def on_mount(self) -> None:
        self.refresh_view(self._view)

    def refresh_view(self, view: ControlView) -> None:
        self._view = view
        self.update(render_light(view))


class GaugeBox(Static):
    """``gauge``: dark placeholder readout (PFD/ND/MCDU displays...)."""

    DEFAULT_CSS = """
    GaugeBox {
        width: auto;
        height: 3;
        border: round $primary-darken-3;
        content-align: center middle;
        text-align: center;
        color: $text-muted;
    }
    """

    def __init__(self, view: ControlView) -> None:
        super().__init__()
        self._view = view
        self.border_title = view.legend
        self.styles.width = max(12, len(view.legend) + 6)

    def on_mount(self) -> None:
        self.refresh_view(self._view)

    def refresh_view(self, view: ControlView) -> None:
        self._view = view
        self.update(render_gauge(view))
