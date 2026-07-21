"""Pure render functions of the per-type widgets, from synthetic views.

Same pattern as test_synoptic_render: no terminal, no app — the render
functions take a frozen ControlView and return rich Text, so the visual
semantics (what lights, what highlights, what stays dark) are asserted
as plain strings.
"""

from __future__ import annotations

from a320_tui.controller import ControlView
from a320_tui.widgets.base import render_local_korry
from a320_tui.widgets.fire_button import render_fire
from a320_tui.widgets.guarded_button import render_guarded
from a320_tui.widgets.key_group import render_key_group
from a320_tui.widgets.lever import render_lever
from a320_tui.widgets.rotary import render_knob, render_knob_pp, render_selector
from a320_tui.widgets.switch import render_switch
from a320_tui.widgets.wheel import render_wheel


def _view(**kwargs) -> ControlView:
    defaults = {"id": "X", "legend": "X", "ctype": "pb"}
    defaults.update(kwargs)
    return ControlView(**defaults)


def test_a_released_korry_is_lights_out():
    text = render_local_korry(_view(lights=("FAULT", "OFF")))
    assert "OFF" not in text.plain
    assert "FAULT" in text.plain  # the dark FAULT slot is drawn, unlit


def test_a_pressed_korry_lights_its_declared_light():
    with_off = render_local_korry(_view(lights=("FAULT", "OFF"), pressed=True))
    assert "OFF" in with_off.plain
    with_on = render_local_korry(_view(lights=("ON",), pressed=True))
    assert "ON" in with_on.plain


def test_a_closed_guard_hides_the_button():
    closed = render_guarded(_view(ctype="pb_guard", lights=("ON",)))
    assert "GUARD" in closed.plain
    assert "closed" in closed.plain
    opened = render_guarded(
        _view(ctype="pb_guard", lights=("ON",), guard_open=True, pressed=True)
    )
    assert "GUARD OPEN" in opened.plain
    assert "ON" in opened.plain


def test_a_fire_pushbutton_shows_popped_state():
    stowed = render_fire(_view(ctype="fire_pb"))
    assert "FIRE" in stowed.plain
    popped = render_fire(_view(ctype="fire_pb", popped_out=True))
    assert "POPPED" in popped.plain


def test_a_selector_highlights_the_live_position():
    text = render_selector(
        _view(ctype="sel", positions=("OFF", "NAV", "ATT"), pos="NAV")
    )
    assert text.plain == "(OFF·NAV·ATT)"
    # NAV must carry the bright style, the others the dark one.
    styled = {span.style: text.plain[span.start:span.end] for span in text.spans}
    assert any("NAV" == chunk for style, chunk in styled.items() if "bold" in str(style))


def test_a_switch_marks_the_live_position_per_line():
    text = render_switch(
        _view(ctype="sw", positions=("ON", "AUTO", "OFF"), pos="AUTO")
    )
    lines = text.plain.splitlines()
    assert lines == ["  ON", "▸ AUTO", "  OFF"]


def test_a_lever_draws_its_detents_top_down():
    text = render_lever(
        _view(
            ctype="lever",
            positions=("MAX_REV", "REV_IDLE", "IDLE", "CL", "FLX_MCT", "TOGA"),
            pos="IDLE",
        )
    )
    lines = text.plain.splitlines()
    assert lines[0].endswith("TOGA")  # last position on top, like the quadrant
    assert lines[-1].endswith("MAX_REV")
    assert any(line.startswith("█") and "IDLE" in line for line in lines)


def test_a_knob_renders_value_and_fill_bar():
    text = render_knob(
        _view(ctype="knob", value=24.0, value_range=(18.0, 30.0))
    )
    assert "24" in text.plain
    assert "█" in text.plain and "░" in text.plain


def test_a_knob_on_a_special_detent_shows_the_label():
    text = render_knob(_view(ctype="knob", pos="AUTO"))
    assert text.plain == "(AUTO)"


def test_an_fcu_knob_shows_the_managed_dot():
    managed = render_knob_pp(
        _view(ctype="knob_pp", value=100.0, value_range=(100.0, 399.0), managed=True)
    )
    assert "managed" in managed.plain
    selected = render_knob_pp(
        _view(ctype="knob_pp", value=250.0, value_range=(100.0, 399.0))
    )
    assert "selected" in selected.plain


def test_a_wheel_shows_signed_deflection():
    assert "+0.30" in render_wheel(_view(ctype="wheel", value=0.3)).plain
    assert "-0.10" in render_wheel(_view(ctype="wheel", value=-0.1)).plain


def test_a_key_group_highlights_the_cursor_key():
    view = _view(ctype="pb_mom", keys=("DIR", "PROG", "PERF"))
    text = render_key_group(view, cursor=1)
    assert "[DIR]" in text.plain and "[PROG]" in text.plain
    bright = [
        text.plain[span.start:span.end]
        for span in text.spans
        if "bold" in str(span.style)
    ]
    assert "[PROG]" in bright
