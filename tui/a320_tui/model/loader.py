"""Loader for the vendored cockpit controls model.

The YAML (``a320-controls-model.yaml``, byte-identical copy of the external
spec) is the single source of truth for every cockpit control. This module
parses it into frozen ``ControlDef`` records, applies the x2/x3 panel
instantiation, and normalizes the handful of irregular entries so the rest
of the TUI can rely on one uniform shape.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from importlib import resources
from typing import Any

import yaml

_MODEL_RESOURCE = "a320-controls-model.yaml"

# The spec marks instantiation only in YAML comments ("# x2: instanciar como
# EFIS_CAPT y EFIS_FO"), which safe_load drops -- the table lives here.
_INSTANTIATION: dict[tuple[str, str], tuple[str, ...]] = {
    ("glareshield", "efis"): ("EFIS_CAPT", "EFIS_FO"),
    ("glareshield", "warnings"): ("WARN_CAPT", "WARN_FO"),
    ("pedestal", "mcdu"): ("MCDU_1", "MCDU_2"),
    ("pedestal", "rmp"): ("RMP_1", "RMP_2", "RMP_3"),
    ("pedestal", "acp"): ("ACP_1", "ACP_2", "ACP_3"),
    ("other", "sidestick"): ("SIDESTICK_CAPT", "SIDESTICK_FO"),
    ("other", "tiller"): ("TILLER_CAPT", "TILLER_FO"),
}

# Controls of an instantiated section that exist only once on the real
# aircraft ("x2 ... salvo AUTOLAND").
_SINGLE_INSTANCE: dict[tuple[str, str], frozenset[str]] = {
    ("glareshield", "warnings"): frozenset({"AUTOLAND"}),
}

# Groups the pedestal mockup draws as individual buttons; every other
# keys:/count: group stays one ControlDef rendered as a composite KeyGroup.
_EXPAND_INDIVIDUALLY = frozenset({"LSK", "SYS_PAGES"})

# LSK count: 12, note "1L-6L, 1R-6R".
_LSK_IDS = tuple(f"LSK_{n}{side}" for side in ("L", "R") for n in range(1, 7))

# def: AUTO on the numeric LDG ELEV knob -- the real panel has an AUTO detent
# outside the numeric range; the string default is kept on purpose.
_DEFAULT_OUTSIDE_POSITIONS = frozenset({"LDG_ELEV"})

_KEY_RANGE = re.compile(r"^([A-Z0-9])-([A-Z0-9])$")


def _as_label(value: Any) -> str:
    # YAML 1.1: an unquoted ON/OFF parses as bool. The vendored spec quotes
    # them, but a future re-sync must not silently produce "True" labels.
    if isinstance(value, bool):
        return "ON" if value else "OFF"
    return str(value)


@dataclass(frozen=True)
class ControlDef:
    """One cockpit control, normalized from the YAML model."""

    id: str
    base_id: str
    panel: str
    section: str
    ctype: str
    lights: tuple[str, ...] = ()
    positions: tuple[str, ...] = ()
    default: str | float | None = None
    value_range: tuple[float, float] | None = None
    keys: tuple[str, ...] = ()
    note: str | None = None
    def_on: bool = False


class CockpitModel:
    """Parsed model: every control, indexed by canonical id."""

    def __init__(self, controls: tuple[ControlDef, ...]) -> None:
        self.controls = controls
        self.by_id: dict[str, ControlDef] = {c.id: c for c in controls}

    def __len__(self) -> int:
        return len(self.controls)


def load_model() -> CockpitModel:
    """Parse the vendored YAML into a validated ``CockpitModel``."""
    raw = _raw_model()
    known_types = frozenset(raw["types"])
    controls: list[ControlDef] = []
    for panel, sections in raw["panels"].items():
        for section, entries in sections.items():
            if isinstance(entries, dict):
                # Placement note (rmp3_acp3), not a list of controls.
                continue
            prefixes = _INSTANTIATION.get((panel, section))
            singles = _SINGLE_INSTANCE.get((panel, section), frozenset())
            for entry in entries:
                for base in _expand_entry(entry, panel, section):
                    if prefixes is None or base.base_id in singles:
                        controls.append(base)
                    else:
                        controls.extend(
                            _instantiate(base, prefix) for prefix in prefixes
                        )
    _validate(controls, known_types)
    return CockpitModel(tuple(controls))


def _raw_model() -> dict[str, Any]:
    text = (resources.files(__package__) / _MODEL_RESOURCE).read_text(
        encoding="utf-8"
    )
    return yaml.safe_load(text)


def _expand_entry(
    entry: dict[str, Any], panel: str, section: str
) -> list[ControlDef]:
    base_id = str(entry["id"])
    if base_id not in _EXPAND_INDIVIDUALLY:
        return [_build(entry, base_id, panel, section)]
    if base_id == "LSK":
        return [
            _build(entry, lsk_id, panel, section, drop_keys=True)
            for lsk_id in _LSK_IDS
        ]
    # SYS_PAGES: one pb per ECAM system page key.
    return [
        _build(entry, f"{base_id}_{key}", panel, section, drop_keys=True)
        for key in entry.get("keys", ())
    ]


def _build(
    entry: dict[str, Any],
    control_id: str,
    panel: str,
    section: str,
    *,
    drop_keys: bool = False,
) -> ControlDef:
    positions = tuple(_as_label(p) for p in entry.get("pos", ()))
    value_range: tuple[float, float] | None = None
    raw_range = entry.get("range")
    if raw_range is not None:
        if all(
            isinstance(v, (int, float)) and not isinstance(v, bool)
            for v in raw_range
        ):
            value_range = (float(raw_range[0]), float(raw_range[1]))
        else:
            # Categorical range ([COLD, HOT]): end labels, not numbers.
            positions = tuple(_as_label(v) for v in raw_range)
    default = entry.get("def")
    if isinstance(default, (int, float)) and not isinstance(default, bool):
        default = float(default)
    elif default is not None:
        default = _as_label(default)
    keys = () if drop_keys else _expand_keys(entry.get("keys", ()))
    return ControlDef(
        id=control_id,
        base_id=control_id,
        panel=panel,
        section=section,
        ctype=str(entry["type"]),
        lights=tuple(_as_label(light) for light in entry.get("lights", ())),
        positions=positions,
        default=default,
        value_range=value_range,
        keys=keys,
        note=entry.get("note"),
        def_on=bool(entry.get("def_on", False)),
    )


def _instantiate(base: ControlDef, prefix: str) -> ControlDef:
    return ControlDef(
        id=f"{prefix}.{base.base_id}",
        base_id=base.base_id,
        panel=base.panel,
        section=base.section,
        ctype=base.ctype,
        lights=base.lights,
        positions=base.positions,
        default=base.default,
        value_range=base.value_range,
        keys=base.keys,
        note=base.note,
        def_on=base.def_on,
    )


def _expand_keys(raw_keys: Any) -> tuple[str, ...]:
    out: list[str] = []
    for key in raw_keys:
        key = _as_label(key)
        match = _KEY_RANGE.match(key)
        if match:
            start, end = match.groups()
            out.extend(chr(c) for c in range(ord(start), ord(end) + 1))
        else:
            out.append(key)
    return tuple(out)


def _validate(controls: list[ControlDef], known_types: frozenset[str]) -> None:
    seen: set[str] = set()
    for control in controls:
        if control.id in seen:
            raise ValueError(f"duplicate control id: {control.id}")
        seen.add(control.id)
        if control.ctype not in known_types:
            raise ValueError(
                f"{control.id}: unknown control type {control.ctype!r}"
            )
        if (
            control.positions
            and isinstance(control.default, str)
            and control.default not in control.positions
            and control.base_id not in _DEFAULT_OUTSIDE_POSITIONS
        ):
            raise ValueError(
                f"{control.id}: default {control.default!r} not in "
                f"positions {control.positions}"
            )
