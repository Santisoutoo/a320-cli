"""Layout coverage: no control of the model may be lost or placed twice.

The zone layouts are hand-arranged data; this is the net under them. As
zones land (overhead now, pedestal/glareshield/main next), their panels
join the covered scope until the union places the whole model.
"""

from __future__ import annotations

import pytest

from a320_tui.layouts import zone_slot_ids
from a320_tui.layouts.overhead import OVERHEAD_ZONE
from a320_tui.model import load_model

ZONES = [OVERHEAD_ZONE]


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


def test_overhead_places_every_overhead_control_plus_rmp3_acp3(model):
    # The overhead quadrant owns both overhead panels and, per the YAML's
    # rmp3_acp3 placement note, the third RMP/ACP instances.
    expected = {
        c.id
        for c in model.controls
        if c.panel in ("overhead_aft", "overhead_fwd")
        or c.id.startswith(("RMP_3.", "ACP_3."))
    }
    placed = set(zone_slot_ids(OVERHEAD_ZONE, model))
    assert placed == expected
