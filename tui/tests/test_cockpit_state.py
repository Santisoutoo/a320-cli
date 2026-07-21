"""Per-type semantics of the local cockpit state (no sim, no Textual)."""

from __future__ import annotations

import pytest

from a320_tui.cockpit_state import CockpitRegistry
from a320_tui.controller import CockpitController
from a320_tui.model import load_model


@pytest.fixture()
def registry():
    return CockpitRegistry(load_model())


@pytest.fixture()
def controller(registry):
    return CockpitController(load_model(), registry)


def test_cold_and_dark_is_released_and_lights_out(registry):
    pressed = [
        cid
        for cid, s in registry.states.items()
        if s.pressed and not registry.definition(cid).def_on
    ]
    assert pressed == []
    assert all(not s.lights_on for s in registry.states.values())
    # def_on survives: the EFIS flight directors start ON.
    assert registry.state("EFIS_CAPT.FD").pressed


def test_a_plain_pushbutton_latches(registry):
    assert registry.press("ELAC_1").changed
    assert registry.state("ELAC_1").pressed
    assert registry.press("ELAC_1").message == "released"


def test_a_momentary_pushbutton_does_not_latch(registry):
    result = registry.press("CVR_TEST")
    assert result.changed
    assert not registry.state("CVR_TEST").pressed


def test_a_guarded_button_needs_two_steps(registry):
    blocked = registry.press("EMER_MAN_ON")
    assert not blocked.changed
    assert "guard" in blocked.message
    assert registry.open_guard("EMER_MAN_ON").changed
    assert registry.press("EMER_MAN_ON").changed
    assert registry.state("EMER_MAN_ON").pressed
    assert registry.close_guard("EMER_MAN_ON").changed


def test_a_fire_pushbutton_pops_out_and_stows(registry):
    assert registry.press("ENG1_FIRE_PB").message == "popped out"
    assert registry.state("ENG1_FIRE_PB").popped_out
    assert registry.press("ENG1_FIRE_PB").message == "stowed"


def test_selectors_cycle_with_end_stops(registry):
    assert registry.state("IR1_MODE").pos == "OFF"
    assert registry.cycle("IR1_MODE", 1).message == "-> NAV"
    assert registry.cycle("IR1_MODE", 1).message == "-> ATT"
    assert not registry.cycle("IR1_MODE", 1).changed  # end stop, no wrap
    assert registry.cycle("IR1_MODE", -2).message == "-> OFF"


def test_move_validates_the_position(registry):
    assert registry.move("ENG_MODE_SEL", "IGN_START").changed
    assert not registry.move("ENG_MODE_SEL", "BOGUS").changed


def test_levers_ride_their_detents(registry):
    assert registry.state("FLAPS_LEVER").pos == "0"
    registry.cycle("FLAPS_LEVER", 1)
    registry.cycle("FLAPS_LEVER", 1)
    assert registry.state("FLAPS_LEVER").pos == "2"


def test_knobs_clamp_to_their_range(registry):
    assert registry.delta("TEMP_COCKPIT", 100.0).message == "-> 30"
    assert not registry.delta("TEMP_COCKPIT", 5.0).changed


def test_ldg_elev_leaves_its_auto_detent_when_turned(registry):
    assert registry.state("LDG_ELEV").pos == "AUTO"
    registry.delta("LDG_ELEV", 500.0)
    state = registry.state("LDG_ELEV")
    assert state.pos is None
    assert state.value == -1500.0  # range floor plus the turn


def test_fcu_knob_push_pull(registry):
    assert registry.push("HDG_KNOB").message == "pushed (managed)"
    assert registry.state("HDG_KNOB").managed
    assert registry.pull("HDG_KNOB").message == "pulled (selected)"
    assert not registry.state("HDG_KNOB").managed


def test_indications_are_not_actuable(registry):
    assert not registry.press("CARGO_SMOKE_FWD").changed  # light
    assert not registry.press("BAT_DISPLAY").changed  # gauge


def test_controller_routes_local_actions_and_builds_views(controller):
    assert controller.actuate("ELAC_1", "press").changed
    view = controller.view("ELAC_1")
    assert view.pressed and not view.wired
    assert view.legend == "ELAC 1"
    assert not controller.actuate("NOT_A_CONTROL", "press").changed
    assert not controller.actuate("ELAC_1", "warp").changed


def test_controller_hands_wired_controls_to_the_sim_setter():
    model = load_model()
    calls: list[tuple[str, object]] = []
    controller = CockpitController(
        model,
        CockpitRegistry(model),
        wired_ids=frozenset({"BAT_1"}),
        set_wired=lambda cid, payload: calls.append((cid, payload)),
    )
    result = controller.actuate("BAT_1", "press", 1.0)
    assert result.changed
    assert calls == [("BAT_1", 1.0)]
    # The local registry must stay untouched for a wired control.
    assert not controller.registry.state("BAT_1").pressed
