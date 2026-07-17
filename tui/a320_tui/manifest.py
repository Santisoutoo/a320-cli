"""The data-driven manifest of what the TUI observes and actuates.

Two halves:

- ``BUTTON_OVERLAYS``: display metadata (legend, Korry light semantics, which
  LVARs feed each light) layered on top of the curated control catalog the core
  already exposes via ``list_controls()``. A control without an overlay still
  gets a generic ON/OFF button, so new Phase-4 controls appear "ugly but
  functional" without touching the TUI.
- ``SYNOPTIC_VARS``: the LVARs the ELEC synoptic page needs.

The union of both is the argument of the selective ``get`` the tick performs —
never ``snapshot()``, which builds a dict of hundreds of vars per call.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional


@dataclass(frozen=True)
class ButtonSpec:
    control: str            # friendly name for sim.set (from list_controls)
    legend: str             # panel legend, e.g. "BAT 1"
    style: str              # "auto_off" | "on_off" | "on_avail" | "world"
    state_var: str          # pushbutton position LVAR (1 = pressed/auto/on)
    fault_var: Optional[str] = None   # amber FAULT light, if the pb has one
    avail_var: Optional[str] = None   # green AVAIL light (ext pwr style)

    def vars(self) -> list[str]:
        out = [self.state_var]
        if self.fault_var:
            out.append(self.fault_var)
        if self.avail_var:
            out.append(self.avail_var)
        return out


# Keyed by the control's friendly name in the curated catalog.
BUTTON_OVERLAYS: dict[str, ButtonSpec] = {
    "bat_1": ButtonSpec(
        "bat_1", "BAT 1", "auto_off",
        "OVHD_ELEC_BAT_1_PB_IS_AUTO", "OVHD_ELEC_BAT_1_PB_HAS_FAULT",
    ),
    "bat_2": ButtonSpec(
        "bat_2", "BAT 2", "auto_off",
        "OVHD_ELEC_BAT_2_PB_IS_AUTO", "OVHD_ELEC_BAT_2_PB_HAS_FAULT",
    ),
    "ext_pwr": ButtonSpec(
        "ext_pwr", "EXT PWR", "on_avail",
        "OVHD_ELEC_EXT_PWR_PB_IS_ON",
        # OVHD_ELEC_EXT_PWR_PB_IS_AVAILABLE never rises in the headless build;
        # the GPU's potential-normal flag is the honest availability signal.
        avail_var="ELEC_EXT_PWR_POTENTIAL_NORMAL",
    ),
    "apu_gen": ButtonSpec(
        "apu_gen", "APU GEN", "on_off",
        "OVHD_ELEC_APU_GEN_PB_IS_ON", "OVHD_ELEC_APU_GEN_PB_HAS_FAULT",
    ),
    "bus_tie": ButtonSpec(
        "bus_tie", "BUS TIE", "auto_off",
        "OVHD_ELEC_BUS_TIE_PB_IS_AUTO", "OVHD_ELEC_BUS_TIE_PB_HAS_FAULT",
    ),
    "gen_1": ButtonSpec(
        "gen_1", "GEN 1", "on_off",
        "OVHD_ELEC_ENG_GEN_1_PB_IS_ON", "OVHD_ELEC_ENG_GEN_1_PB_HAS_FAULT",
    ),
    "gen_2": ButtonSpec(
        "gen_2", "GEN 2", "on_off",
        "OVHD_ELEC_ENG_GEN_2_PB_IS_ON", "OVHD_ELEC_ENG_GEN_2_PB_HAS_FAULT",
    ),
    "ext_pwr_avail": ButtonSpec(
        "ext_pwr_avail", "GPU", "world", "EXT_PWR_AVAIL:1",
    ),
}


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
    for spec in button_specs(controls):
        for name in spec.vars():
            seen.setdefault(name)
    return list(seen)
