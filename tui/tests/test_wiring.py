"""Anti-drift invariants of the YAML<->sim wiring.

Three promises: every wired id is a real control of the vendored model,
every LVAR a wired spec reads exists in the live registry, and no cockpit
control of the curated catalog is left without wiring. A vendor bump or a
YAML re-sync that breaks any of these fails here, not silently in a panel.
"""

from __future__ import annotations

import a320_sim
import pytest

from a320_tui.model import load_model
from a320_tui.wiring import WIRING, WORLD_SPECS


@pytest.fixture(scope="module")
def sim():
    s = a320_sim.Sim()
    s.step(100)
    return s


@pytest.fixture(scope="module")
def registered(sim):
    return set(sim.list_variables())


def test_every_wired_id_exists_in_the_model():
    model = load_model()
    missing = [cid for cid in WIRING if cid not in model.by_id]
    assert missing == []


def test_every_wired_var_is_registered(registered):
    specs = list(WIRING.values()) + list(WORLD_SPECS.values())
    missing = [
        (spec.legend, var)
        for spec in specs
        for var in spec.vars()
        if var not in registered
    ]
    assert missing == []


def test_every_cockpit_catalog_control_is_wired(sim):
    wired_controls = {spec.control for spec in WIRING.values()}
    orphans = [
        c["name"]
        for c in sim.list_controls()
        if c["domain"] != "world" and c["name"] not in wired_controls
    ]
    assert orphans == [], (
        "curated catalog grew: wire these ids in wiring.WIRING so the "
        f"cockpit does not silently drop them: {orphans}"
    )


def test_apu_raw_lvars_round_trip(sim):
    spec = WIRING["APU_MASTER_SW"]
    sim.set(spec.control, 1.0)
    sim.step(100)
    assert sim.get([spec.state_var])[spec.state_var] == 1.0
    sim.set(spec.control, 0.0)
    sim.step(100)
    assert sim.get([spec.state_var])[spec.state_var] == 0.0
