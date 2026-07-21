"""Quadrant: one cell of the 2x2 cockpit grid, scrollable on both axes.

The four zones are visible at once (the vertical stack of the real cockpit
is ~95 rows and does not fit any terminal); each quadrant keeps its
content at natural width and scrolls independently. F1-F4 focus a
quadrant; once focused, the arrow keys scroll it and Tab walks into its
controls.
"""

from __future__ import annotations

from textual.containers import ScrollableContainer


class Quadrant(ScrollableContainer, can_focus=True):
    DEFAULT_CSS = """
    Quadrant {
        border: round $primary-darken-2;
        border-title-color: $text-muted;
        scrollbar-size-vertical: 1;
        scrollbar-size-horizontal: 1;
    }
    Quadrant:focus {
        border: round $accent;
    }
    Quadrant:focus-within {
        border: round $accent-darken-2;
    }
    """

    def __init__(self, title: str, *, id: str | None = None) -> None:
        super().__init__(id=id)
        self.border_title = title
