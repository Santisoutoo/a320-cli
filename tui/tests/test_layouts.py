"""Layout coverage: no control of the model may be lost or placed twice.

The zone layouts are hand-arranged data; this is the net under them. As
zones land (overhead now, pedestal/glareshield/main next), their panels
join the covered scope until the union places the whole model.
"""

from __future__ import annotations

import pytest

from a320_tui.layouts import zone_slot_ids
from a320_tui.layouts.glareshield import GLARESHIELD_ZONE
from a320_tui.layouts.main_panel import MAIN_PANEL_ZONE
from a320_tui.layouts.overhead import OVERHEAD_ZONE
from a320_tui.layouts.pedestal import PEDESTAL_ZONE
from a320_tui.model import load_model

ZONES = [OVERHEAD_ZONE, GLARESHIELD_ZONE, MAIN_PANEL_ZONE, PEDESTAL_ZONE]


@pytest.fixture(scope="module")
def model():
    return load_model()


def _all_placed(model) -> list[str]:
    ids: list[str] = []
    for zone in ZONES:
        ids.extend(zone_slot_ids(zone, model))
    return ids


def test_no_control_is_placed_twice(model):
    placed = _all_placed(model)
    dupes = {cid for cid in placed if placed.count(cid) > 1}
    assert dupes == set()


def test_every_slot_is_a_real_model_control(model):
    unknown = [cid for cid in _all_placed(model) if cid not in model.by_id]
    assert unknown == []


def test_the_zones_place_the_entire_model_exactly_once(model):
    # RMP_3/ACP_3 belong to the pedestal panel in the YAML but are placed
    # on the overhead (the rmp3_acp3 note); coverage is asserted globally:
    # all four zones together place every control of the model, no strays.
    expected = set(model.by_id)
    placed = set(_all_placed(model))
    missing = expected - placed
    stray = placed - expected
    assert missing == set(), f"controls lost by the layouts: {sorted(missing)}"
    assert stray == set(), f"placed but not in the model: {sorted(stray)}"
