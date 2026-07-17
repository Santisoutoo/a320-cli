"""OverheadPanel: the ELEC overhead, built data-driven from the catalog.

Iterates the ``ButtonSpec`` list (curated catalog + display overlays) and
mounts one ``KorryButton`` per control. World-domain controls (scenario knobs
like "GPU plugged") get their own section under the cockpit rows.
"""

from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Grid, Vertical
from textual.widgets import Label

from a320_tui.manifest import ButtonSpec
from a320_tui.state import SimState
from a320_tui.widgets.korry_button import KorryButton


class OverheadPanel(Vertical):
    DEFAULT_CSS = """
    OverheadPanel {
        width: 50;
        border: round $primary-darken-2;
        padding: 0 1;
    }
    OverheadPanel Grid {
        grid-size: 3;
        grid-rows: 4;
        grid-columns: 15;
        height: auto;
    }
    OverheadPanel .section {
        color: $text-muted;
        margin: 1 0 0 0;
    }
    """

    def __init__(self, specs: list[ButtonSpec], world_specs: list[ButtonSpec]) -> None:
        super().__init__()
        self.border_title = "OVERHEAD · ELEC"
        self._specs = specs
        self._world_specs = world_specs
        self._buttons: list[KorryButton] = []

    def compose(self) -> ComposeResult:
        with Grid():
            for spec in self._specs:
                button = KorryButton(spec)
                self._buttons.append(button)
                yield button
        if self._world_specs:
            yield Label("— WORLD (scenario) —", classes="section")
            with Grid():
                for spec in self._world_specs:
                    button = KorryButton(spec)
                    self._buttons.append(button)
                    yield button

    def update_state(self, state: SimState) -> None:
        for button in self._buttons:
            button.update_state(state)
