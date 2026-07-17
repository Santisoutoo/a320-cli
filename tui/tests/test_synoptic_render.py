"""The synoptic render is a pure function of SimState: test it synthetically."""

from a320_tui.state import SimState
from a320_tui.widgets.elec_synoptic import _AMBER, _GREEN, render_elec_synoptic


def _styles_of(text, needle):
    """Styles applied to every span whose text contains the needle."""
    plain = text.plain
    out = []
    for span in text.spans:
        if needle in plain[span.start : span.end]:
            out.append(str(span.style))
    return out


def test_cold_and_dark_is_amber():
    state = SimState(t=0.0, vars={})
    text = render_elec_synoptic(state)
    assert _AMBER in _styles_of(text, "AC 1")
    assert _AMBER in _styles_of(text, "DC BAT")
    assert _GREEN not in _styles_of(text, "AC 1")


def test_powered_buses_render_green():
    state = SimState(
        t=1.0,
        vars={
            "ELEC_DC_BAT_BUS_IS_POWERED": 1.0,
            "ELEC_AC_1_BUS_IS_POWERED": 1.0,
            "ELEC_TR_1_POTENTIAL_NORMAL": 1.0,
            "ELEC_EXT_PWR_POTENTIAL_NORMAL": 1.0,
        },
    )
    text = render_elec_synoptic(state)
    assert _GREEN in _styles_of(text, "DC BAT")
    assert _GREEN in _styles_of(text, "AC 1")
    assert _GREEN in _styles_of(text, "TR 1")
    assert _GREEN in _styles_of(text, "EXT PWR")
    # Unpowered neighbours stay amber.
    assert _AMBER in _styles_of(text, "AC 2")


def test_render_is_rectangular():
    # Box-drawing art degrades badly if a line drifts; keep rows aligned.
    text = render_elec_synoptic(SimState(t=0.0, vars={}))
    lines = [l for l in text.plain.splitlines() if l.strip()]
    widths = {len(l.rstrip()) for l in lines}
    assert max(widths) - min(widths) <= 14, f"ragged synoptic widths: {sorted(widths)}"
