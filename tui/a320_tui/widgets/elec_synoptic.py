"""ElecSynoptic: an SD-style ELEC page drawn with box-drawing characters.

Rendering is a pure function of ``SimState`` (testable without a terminal).
Conventions follow the real system display: a bus box is green when powered,
amber when not; sources (generators, ext pwr) and TRs are green when their
output is normal, dim when dead. Links between boxes are green only when both
ends are alive — an honest approximation of flow, not a contactor-accurate
routing (documented in docs/faseT-tui.md).

Layout (top to bottom, like the real page: batteries, DC, TRs, AC, sources):

       ┌────────┐   ┌────────┐   ┌────────┐
       │ BAT 1  ├───┤ DC BAT ├───┤ BAT 2  │
       └────────┘   └───┬────┘   └────────┘
           ┌────────────┼────────────┐
       ┌───┴────┐   ┌───┴────┐   ┌───┴────┐
       │  DC 1  │   │ DC ESS │   │  DC 2  │
       ...
"""

from __future__ import annotations

from rich.text import Text
from textual.widgets import Static

from a320_tui.state import SimState

_GREEN = "green3"
_AMBER = "orange3"
_DIM = "grey35"

_INDENT = "   "
_GAP = "    "


def _bus(on: bool) -> str:
    return _GREEN if on else _AMBER


def _live(on: bool) -> str:
    return _GREEN if on else _DIM


def _row(cells: list[tuple[str, str]], gaps: "list[tuple[str, str]] | None" = None) -> Text:
    """One rendered line: indent + cell/gap/cell/gap/cell, each styled."""
    if gaps is None:
        gaps = [(_GAP, _DIM), (_GAP, _DIM)]
    line = Text(_INDENT)
    for i, (chunk, style) in enumerate(cells):
        line.append(chunk, style)
        if i < len(gaps):
            line.append(gaps[i][0], gaps[i][1])
    line.append("\n")
    return line


def render_elec_synoptic(state: SimState) -> Text:
    v = state.is_on

    bat_bus = v("ELEC_DC_BAT_BUS_IS_POWERED")
    dc1, dc2 = v("ELEC_DC_1_BUS_IS_POWERED"), v("ELEC_DC_2_BUS_IS_POWERED")
    dc_ess = v("ELEC_DC_ESS_BUS_IS_POWERED")
    ac1, ac2 = v("ELEC_AC_1_BUS_IS_POWERED"), v("ELEC_AC_2_BUS_IS_POWERED")
    ac_ess = v("ELEC_AC_ESS_BUS_IS_POWERED")
    hot1, hot2 = v("ELEC_DC_HOT_1_BUS_IS_POWERED"), v("ELEC_DC_HOT_2_BUS_IS_POWERED")
    tr1, tr2 = v("ELEC_TR_1_POTENTIAL_NORMAL"), v("ELEC_TR_2_POTENTIAL_NORMAL")
    ess_tr = v("ELEC_TR_3_POTENTIAL_NORMAL")
    gen1 = v("ELEC_ENG_GEN_1_POTENTIAL_NORMAL")
    gen2 = v("ELEC_ENG_GEN_2_POTENTIAL_NORMAL")
    apu_gen = v("ELEC_APU_GEN_1_POTENTIAL_NORMAL")
    ext = v("ELEC_EXT_PWR_POTENTIAL_NORMAL")

    text = Text()

    # Batteries + battery bus. Hot buses are always alive with batteries in:
    # the BAT boxes track them; the tie link tracks the battery bus.
    tie_l = (_GAP.replace(" ", "─"), _live(hot1 and bat_bus))
    tie_r = (_GAP.replace(" ", "─"), _live(hot2 and bat_bus))
    text.append_text(_row(
        [("┌────────┐", _bus(hot1)), ("┌────────┐", _bus(bat_bus)), ("┌────────┐", _bus(hot2))]))
    text.append_text(_row(
        [("│ BAT 1  ├", _bus(hot1)), ("┤ DC BAT ├", _bus(bat_bus)), ("┤ BAT 2  │", _bus(hot2))],
        [tie_l, tie_r]))
    text.append_text(_row(
        [("└────────┘", _bus(hot1)), ("└───┬────┘", _bus(bat_bus)), ("└────────┘", _bus(hot2))]))

    # Battery bus down to the DC network.
    spread = Text(_INDENT + "    ")
    spread.append("┌────────────", _live(bat_bus and dc1))
    spread.append("┼", _live(bat_bus))
    spread.append("────────────┐", _live(bat_bus and dc2))
    spread.append("\n")
    text.append_text(spread)

    # DC buses.
    text.append_text(_row(
        [("┌───┴────┐", _bus(dc1)), ("┌───┴────┐", _bus(dc_ess)), ("┌───┴────┐", _bus(dc2))]))
    text.append_text(_row(
        [("│  DC 1  │", _bus(dc1)), ("│ DC ESS │", _bus(dc_ess)), ("│  DC 2  │", _bus(dc2))]))
    text.append_text(_row(
        [("└───┬────┘", _bus(dc1)), ("└───┬────┘", _bus(dc_ess)), ("└───┬────┘", _bus(dc2))]))

    # TRs between the DC and AC layers.
    text.append_text(_row(
        [("  [ TR 1 ]", _live(tr1)), ("  [ESS TR]", _live(ess_tr)), ("  [ TR 2 ]", _live(tr2))]))

    # AC buses.
    text.append_text(_row(
        [("┌───┴────┐", _bus(ac1)), ("┌───┴────┐", _bus(ac_ess)), ("┌───┴────┐", _bus(ac2))]))
    text.append_text(_row(
        [("│  AC 1  │", _bus(ac1)), ("│ AC ESS │", _bus(ac_ess)), ("│  AC 2  │", _bus(ac2))]))
    text.append_text(_row(
        [("└───┬────┘", _bus(ac1)), ("└────────┘", _bus(ac_ess)), ("└───┬────┘", _bus(ac2))]))

    # Sources at the bottom, like the real page.
    text.append_text(_row(
        [("  GEN 1   ", _live(gen1)), ("          ", _DIM), ("  GEN 2   ", _live(gen2))]))
    tail = Text(_INDENT + "          ")
    tail.append("EXT PWR", _live(ext))
    tail.append("   ")
    tail.append("APU GEN", _live(apu_gen))
    text.append_text(tail)

    return text


class ElecSynoptic(Static):
    DEFAULT_CSS = """
    ElecSynoptic {
        border: round $primary-darken-2;
        padding: 1 1;
        height: 1fr;
        overflow: auto auto;
    }
    """

    def __init__(self) -> None:
        super().__init__()
        self.border_title = "SD · ELEC"

    def update_state(self, state: SimState) -> None:
        self.update(render_elec_synoptic(state))
