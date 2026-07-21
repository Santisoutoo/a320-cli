"""Zone layouts: the panel geometry as data, transcribed from the mockups.

A zone (OVERHEAD, PEDESTAL, GLARESHIELD, MAIN) is columns of sections; a
section is rows of slots. A slot is a canonical model id, or one of the
special kinds the 35VU already used:

- ``mimic:<name>``  painted diagram, not an instrument (bus mimic)
- ``prop:<legend>`` real-panel position the sim does not model, inert

Sections come in two flavors: ``Section`` with hand-transcribed rows (used
where the mockup fixes the geometry, e.g. the ELEC 35VU) and ``AutoSection``,
which lays out every control of one YAML (panel, section) — optionally one
instance — in rows of N. Auto is the default on purpose: the mockups
compress several controls per cell anyway, and enumerating ids by hand for
~400 controls is exactly how one silently goes missing. The coverage test
(`tests/test_layouts.py`) guarantees the union places every model id
exactly once.
"""

from __future__ import annotations

from dataclasses import dataclass

from a320_tui.model import CockpitModel

_SPECIAL_PREFIXES = ("mimic:", "prop:")


@dataclass(frozen=True)
class Section:
    title: str
    rows: tuple[tuple[str, ...], ...]


@dataclass(frozen=True)
class AutoSection:
    """All controls of one YAML (panel, section), laid out in rows of N."""

    title: str
    panel: str
    section: str
    prefix: str | None = None  # instance selector, e.g. "RMP_3"
    per_row: int = 4


@dataclass(frozen=True)
class ZoneSpec:
    name: str
    columns: tuple[tuple[Section | AutoSection, ...], ...]


def resolve_section(
    section: Section | AutoSection, model: CockpitModel
) -> Section:
    if isinstance(section, Section):
        return section
    ids = [
        c.id
        for c in model.controls
        if c.panel == section.panel
        and c.section == section.section
        and _instance_matches(c.id, section.prefix)
    ]
    per_row = section.per_row
    rows = tuple(
        tuple(ids[i : i + per_row]) for i in range(0, len(ids), per_row)
    )
    return Section(section.title, rows)


def _instance_matches(control_id: str, prefix: str | None) -> bool:
    if prefix is None:
        return "." not in control_id
    return control_id.startswith(f"{prefix}.")


def zone_slot_ids(zone: ZoneSpec, model: CockpitModel) -> list[str]:
    """Every model id the zone places (special slots excluded)."""
    ids: list[str] = []
    for column in zone.columns:
        for section in column:
            for row in resolve_section(section, model).rows:
                ids.extend(
                    slot
                    for slot in row
                    if not slot.startswith(_SPECIAL_PREFIXES)
                )
    return ids
