"""The vendored YAML model parses into a complete, well-formed control set.

These tests pin the contract of ``load_model()``: the x2/x3 instantiation
that the spec only marks in comments, the LSK/SYS_PAGES expansion, the
normalization of the YAML's irregular entries, and the global uniqueness
of canonical ids. If a re-sync of the vendored YAML changes any of this,
it must fail here, not deep inside a widget.
"""

from __future__ import annotations

import pytest

from a320_tui.model import load_model


@pytest.fixture(scope="module")
def model():
    return load_model()


def test_the_model_loads_and_has_the_expected_size(model):
    # 423 = every control in the YAML after x2/x3 instantiation plus the
    # LSK (12 per MCDU) and SYS_PAGES (12) expansions. A re-sync of the
    # vendored spec that changes this number is a conscious event.
    assert len(model) == 423


def test_ids_are_globally_unique(model):
    ids = [c.id for c in model.controls]
    assert len(ids) == len(set(ids))


def test_duplicated_panels_are_instantiated(model):
    assert "EFIS_CAPT.FD" in model.by_id
    assert "EFIS_FO.FD" in model.by_id
    assert {f"RMP_{n}.VHF1" for n in (1, 2, 3)} <= model.by_id.keys()
    assert {f"ACP_{n}.VOICE" for n in (1, 2, 3)} <= model.by_id.keys()
    assert "SIDESTICK_CAPT.PTT" in model.by_id
    assert "TILLER_FO.PEDAL_DISC" in model.by_id


def test_autoland_is_a_single_instance(model):
    autoland = [c for c in model.controls if c.base_id == "AUTOLAND"]
    assert [c.id for c in autoland] == ["AUTOLAND"]
    # Its siblings in the warnings section do get duplicated.
    assert "WARN_CAPT.MASTER_WARN" in model.by_id
    assert "WARN_FO.MASTER_CAUT" in model.by_id


def test_lsk_and_sys_pages_expand_to_individual_buttons(model):
    lsks = [c for c in model.controls if c.base_id.startswith("LSK_")]
    assert len(lsks) == 24  # 12 per MCDU
    assert "MCDU_1.LSK_1L" in model.by_id
    assert "MCDU_2.LSK_6R" in model.by_id
    pages = [c for c in model.controls if c.base_id.startswith("SYS_PAGES_")]
    assert len(pages) == 12
    assert model.by_id["SYS_PAGES_ENG"].lights == ("ON",)


def test_key_groups_stay_composite_with_expanded_ranges(model):
    alpha = model.by_id["MCDU_1.ALPHA"]
    assert "A" in alpha.keys and "Z" in alpha.keys and "OVFY" in alpha.keys
    assert len(alpha.keys) == 30  # A-Z + SP, OVFY, CLR, SLASH
    xpdr = model.by_id["XPDR_KEYS"]
    assert xpdr.keys == ("0", "1", "2", "3", "4", "5", "6", "7", "CLR")


def test_cold_and_dark_defaults_survive_normalization(model):
    assert model.by_id["ELT"].default == "ARM"
    assert model.by_id["PARK_BRK"].default == "ON"
    assert model.by_id["EFIS_CAPT.ND_RANGE"].default == "80"
    assert model.by_id["EFIS_FO.FD"].def_on is True
    assert model.by_id["IR1_MODE"].default == "OFF"


def test_no_boolean_leaks_from_yaml_11(model):
    for control in model.controls:
        for label in (*control.positions, *control.lights, *control.keys):
            assert label not in ("True", "False"), control.id


def test_irregular_entries_are_normalized(model):
    # def: AUTO on a numeric knob (special detent outside the range).
    ldg = model.by_id["LDG_ELEV"]
    assert ldg.default == "AUTO"
    assert ldg.value_range == (-2000.0, 14000.0)
    # Categorical range becomes end labels, not a numeric range.
    cargo = model.by_id["CARGO_HEAT_FWD_TEMP"]
    assert cargo.value_range is None
    assert cargo.positions == ("COLD", "HOT")
    # The rmp3_acp3 placement note is not a control.
    assert not any(c.section == "rmp3_acp3" for c in model.controls)


def test_every_type_comes_from_the_declared_set(model):
    declared = {
        "pb", "pb_mom", "pb_guard", "fire_pb", "sel", "knob", "knob_pp",
        "sw", "lever", "wheel", "light", "gauge",
    }
    assert {c.ctype for c in model.controls} <= declared
