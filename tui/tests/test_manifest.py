"""Anti-drift: every var the TUI manifest wants must exist in the registry.

The Python twin of the core's ``every_catalog_lvar_is_registered_after_a_tick``:
if a vendor update renames an LVAR the synoptic depends on, this fails loudly
instead of the TUI silently rendering zeros.
"""

import a320_sim
import pytest

from a320_tui.manifest import (
    EXTRA_PANEL_SPECS,
    PANEL_LEFT_STACK,
    PANEL_SOURCES_ROW,
    PANEL_TOP_ROW,
    PLACED_CONTROLS,
    button_specs,
    manifest_vars,
)


@pytest.fixture(scope="module")
def sim():
    s = a320_sim.Sim()
    s.step(200)
    return s


def test_every_manifest_var_is_registered(sim):
    wanted = manifest_vars(sim.list_controls())
    known = set(sim.list_variables())
    missing = [v for v in wanted if v not in known]
    assert not missing, f"manifest vars missing from the registry: {missing}"


def test_manifest_get_resolves(sim):
    wanted = manifest_vars(sim.list_controls())
    state = sim.get(wanted)
    assert set(state) == set(wanted)


def test_every_catalog_control_gets_a_button(sim):
    controls = sim.list_controls()
    specs = button_specs(controls)
    assert len(specs) == len(controls)
    assert {s.control for s in specs} == {c["name"] for c in controls}


def test_the_35vu_geometry_covers_todays_cockpit_catalog(sim):
    """Every cockpit control the catalog exposes today has a slot on the panel.

    If this fails after a catalog change, either the geometry gained a slot for
    the new control or it deliberately goes to the OTHER section — but never
    silently. (World-domain controls live in their own section by design.)
    """
    cockpit = {c["name"] for c in sim.list_controls() if c["domain"] == "cockpit"}
    unplaced = cockpit - PLACED_CONTROLS
    assert not unplaced, (
        f"cockpit controls without a 35VU slot (they will render under OTHER): "
        f"{sorted(unplaced)}"
    )


def test_extra_panel_hardware_exists_in_the_registry(sim):
    """The raw-LVAR panel hardware (AC ESS FEED, COMMERCIAL, GALY & CAB) is real.

    These bypass the curated catalog on purpose (D-008 raw path), so the
    catalog's own anti-drift test does not cover them — this one does. If a
    vendor bump renames one, the button would silently read 0 and write into a
    freshly minted variable nobody consumes.
    """
    known = set(sim.list_variables())
    for key, spec in EXTRA_PANEL_SPECS.items():
        for var in spec.vars():
            assert var in known, f"{key}: '{var}' not in the registry"
        # And the write path accepts them (raises SimError otherwise).
        sim.set(spec.state_var, sim.get([spec.state_var])[spec.state_var])


def test_panel_slots_are_well_formed():
    kinds = {"catalog", "extra", "bat_display", "prop"}
    for slot in PANEL_TOP_ROW + PANEL_LEFT_STACK + PANEL_SOURCES_ROW:
        kind, _, name = slot.partition(":")
        assert kind in kinds and name, f"malformed slot: {slot!r}"
        if kind == "extra":
            assert name in EXTRA_PANEL_SPECS, f"unknown extra slot: {slot!r}"
