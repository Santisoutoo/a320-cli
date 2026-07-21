"""The data-driven manifest of what the TUI observes per tick.

The wiring itself (which YAML control maps to which sim name and LVARs)
lives in ``wiring.py``, keyed by the model's canonical ids. This module
keeps the observation side: the synoptic/voltmeter var lists and
``manifest_vars()``, the union everything one tick needs to read via the
selective ``get`` — never ``snapshot()``, which builds a dict of hundreds
of vars per call.

``BUTTON_OVERLAYS``/``EXTRA_PANEL_SPECS`` are compatibility views of the
wiring keyed by catalog friendly name / legacy extra name; the zone
layouts themselves consume ``WIRING`` directly (see ``widgets/zone_panel``).
"""

from __future__ import annotations

from a320_tui.wiring import WIRING, WORLD_SPECS, ButtonSpec

__all__ = [
    "BAT_DISPLAY_VARS",
    "BUTTON_OVERLAYS",
    "ButtonSpec",
    "EXTRA_PANEL_SPECS",
    "SYNOPTIC_VARS",
    "button_specs",
    "manifest_vars",
]

# Wired specs whose `control` is a curated-catalog friendly name (lowercase),
# keyed by that name — plus the world/scenario specs, which share the shape.
BUTTON_OVERLAYS: dict[str, ButtonSpec] = {
    spec.control: spec
    for spec in WIRING.values()
    if not spec.control.startswith("OVHD_")
} | WORLD_SPECS

# Wired specs that actuate through the documented raw-LVAR path (D-008/D-009),
# keyed by their legacy manifest name for the 35VU geometry below.
EXTRA_PANEL_SPECS: dict[str, ButtonSpec] = {
    "ac_ess_feed": WIRING["AC_ESS_FEED"],
    "commercial": WIRING["COMMERCIAL"],
    "galy_and_cab": WIRING["GALY_CAB"],
}


# The battery voltmeters between the BAT pushbuttons (live on the real panel).
BAT_DISPLAY_VARS = ["ELEC_BAT_1_POTENTIAL", "ELEC_BAT_2_POTENTIAL"]


def button_specs(controls: list[dict]) -> list[ButtonSpec]:
    """Build the panel from the core's curated catalog, in catalog order.

    Controls with an overlay use it; the rest get a generic world/cockpit
    toggle over their own LVAR, so the panel never silently drops a control.
    """
    specs = []
    for c in controls:
        overlay = BUTTON_OVERLAYS.get(c["name"])
        if overlay is not None:
            specs.append(overlay)
        else:
            style = "world" if c["domain"] == "world" else "on_off"
            specs.append(ButtonSpec(c["name"], c["name"].upper(), style, c["lvar"]))
    return specs


# LVARs the ELEC synoptic renders. All confirmed present in the registry
# (tests/test_manifest.py keeps this list honest against vendor updates).
SYNOPTIC_VARS = [
    "ELEC_AC_1_BUS_IS_POWERED",
    "ELEC_AC_2_BUS_IS_POWERED",
    "ELEC_AC_ESS_BUS_IS_POWERED",
    "ELEC_DC_1_BUS_IS_POWERED",
    "ELEC_DC_2_BUS_IS_POWERED",
    "ELEC_DC_ESS_BUS_IS_POWERED",
    "ELEC_DC_BAT_BUS_IS_POWERED",
    "ELEC_DC_HOT_1_BUS_IS_POWERED",
    "ELEC_DC_HOT_2_BUS_IS_POWERED",
    "ELEC_TR_1_POTENTIAL_NORMAL",
    "ELEC_TR_2_POTENTIAL_NORMAL",
    "ELEC_TR_3_POTENTIAL_NORMAL",
    "ELEC_ENG_GEN_1_POTENTIAL_NORMAL",
    "ELEC_ENG_GEN_2_POTENTIAL_NORMAL",
    "ELEC_APU_GEN_1_POTENTIAL_NORMAL",
    "ELEC_EXT_PWR_POTENTIAL_NORMAL",
    "ELEC_STAT_INV_POTENTIAL_NORMAL",
    "EXT_PWR_AVAIL:1",
]


def manifest_vars(controls: list[dict]) -> list[str]:
    """Union of everything one tick needs to read, stable order, no dupes."""
    seen: dict[str, None] = {}
    for name in SYNOPTIC_VARS:
        seen.setdefault(name)
    for name in BAT_DISPLAY_VARS:
        seen.setdefault(name)
    for spec in button_specs(controls):
        for name in spec.vars():
            seen.setdefault(name)
    for spec in WIRING.values():
        for name in spec.vars():
            seen.setdefault(name)
    return list(seen)
