"""Declarative wiring between the YAML cockpit model and the sim.

``WIRING`` is keyed by the model's canonical control id. A control listed
here is *wired*: its widget renders from ``SimState`` and its actuation
goes through ``SimBridge.set`` — either as a curated-catalog friendly name
or through the documented raw-LVAR path (D-008/D-009). Every other control
of the model operates on local state only (``cockpit_state.py``).

Growing the sim's reach (Phase 4...) means adding an entry here; the
anti-drift tests in ``tests/test_wiring.py`` guarantee that every wired
var exists in the live registry and that no catalog control is orphaned.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional


@dataclass(frozen=True)
class ButtonSpec:
    control: str            # name for sim.set: friendly (catalog) or raw LVAR
    legend: str             # panel legend, e.g. "BAT 1"
    style: str              # korry light semantics, see KorryButton
    state_var: str          # pushbutton position LVAR (1 = pressed/auto/on/normal)
    fault_var: Optional[str] = None   # amber FAULT light, if the pb has one
    avail_var: Optional[str] = None   # green AVAIL light (ext pwr style)

    def vars(self) -> list[str]:
        out = [self.state_var]
        if self.fault_var:
            out.append(self.fault_var)
        if self.avail_var:
            out.append(self.avail_var)
        return out


WIRING: dict[str, ButtonSpec] = {
    # ---- ELEC: curated catalog (friendly names) -------------------------
    "BAT_1": ButtonSpec(
        "bat_1", "BAT 1", "auto_off",
        "OVHD_ELEC_BAT_1_PB_IS_AUTO", "OVHD_ELEC_BAT_1_PB_HAS_FAULT",
    ),
    "BAT_2": ButtonSpec(
        "bat_2", "BAT 2", "auto_off",
        "OVHD_ELEC_BAT_2_PB_IS_AUTO", "OVHD_ELEC_BAT_2_PB_HAS_FAULT",
    ),
    "EXT_PWR": ButtonSpec(
        "ext_pwr", "EXT PWR", "on_avail",
        "OVHD_ELEC_EXT_PWR_PB_IS_ON",
        # OVHD_ELEC_EXT_PWR_PB_IS_AVAILABLE never rises in the headless build;
        # the GPU's potential-normal flag is the honest availability signal.
        avail_var="ELEC_EXT_PWR_POTENTIAL_NORMAL",
    ),
    "APU_GEN": ButtonSpec(
        "apu_gen", "APU GEN", "on_off",
        "OVHD_ELEC_APU_GEN_PB_IS_ON", "OVHD_ELEC_APU_GEN_PB_HAS_FAULT",
    ),
    "BUS_TIE": ButtonSpec(
        "bus_tie", "BUS TIE", "auto_off",
        "OVHD_ELEC_BUS_TIE_PB_IS_AUTO", "OVHD_ELEC_BUS_TIE_PB_HAS_FAULT",
    ),
    "GEN_1": ButtonSpec(
        "gen_1", "GEN 1", "on_off",
        "OVHD_ELEC_ENG_GEN_1_PB_IS_ON", "OVHD_ELEC_ENG_GEN_1_PB_HAS_FAULT",
    ),
    "GEN_2": ButtonSpec(
        "gen_2", "GEN 2", "on_off",
        "OVHD_ELEC_ENG_GEN_2_PB_IS_ON", "OVHD_ELEC_ENG_GEN_2_PB_HAS_FAULT",
    ),
    # ---- ELEC: FBW-modeled hardware outside the catalog (raw LVARs) -----
    # Verified against the vendored Rust (a320_systems/src/electrical/mod.rs):
    #   :283  ac_ess_feed  NormalAltnFaultPushButton::new_normal("ELEC_AC_ESS_FEED")
    #   :284  galy_and_cab AutoOffFaultPushButton::new_auto("ELEC_GALY_AND_CAB")
    #   :286  commercial   OnOffFaultPushButton::new_on("ELEC_COMMERCIAL")
    "AC_ESS_FEED": ButtonSpec(
        "OVHD_ELEC_AC_ESS_FEED_PB_IS_NORMAL", "AC ESS FEED", "normal_altn",
        "OVHD_ELEC_AC_ESS_FEED_PB_IS_NORMAL",
        "OVHD_ELEC_AC_ESS_FEED_PB_HAS_FAULT",
    ),
    "COMMERCIAL": ButtonSpec(
        "OVHD_ELEC_COMMERCIAL_PB_IS_ON", "COMMERCIAL", "on_off",
        "OVHD_ELEC_COMMERCIAL_PB_IS_ON",
        "OVHD_ELEC_COMMERCIAL_PB_HAS_FAULT",
    ),
    "GALY_CAB": ButtonSpec(
        "OVHD_ELEC_GALY_AND_CAB_PB_IS_AUTO", "GALY & CAB", "auto_off",
        "OVHD_ELEC_GALY_AND_CAB_PB_IS_AUTO",
        "OVHD_ELEC_GALY_AND_CAB_PB_HAS_FAULT",
    ),
    # ---- APU: startable on the ground, raw LVARs (not in the catalog yet;
    # same names the MCP harness uses for --start apu-running) -------------
    "APU_MASTER_SW": ButtonSpec(
        "OVHD_APU_MASTER_SW_PB_IS_ON", "APU MASTER SW", "fault_on",
        "OVHD_APU_MASTER_SW_PB_IS_ON", "OVHD_APU_MASTER_SW_PB_HAS_FAULT",
    ),
    "APU_START": ButtonSpec(
        "OVHD_APU_START_PB_IS_ON", "APU START", "on_avail",
        "OVHD_APU_START_PB_IS_ON",
        avail_var="OVHD_APU_START_PB_IS_AVAILABLE",
    ),
}


# Scenario knobs, not cockpit hardware: the model has no YAML id for the
# GPU because the crew cannot plug one in — it belongs to the world/scenario
# section next to the synoptic, not to a panel (D-009 cockpit/world split).
WORLD_SPECS: dict[str, ButtonSpec] = {
    "ext_pwr_avail": ButtonSpec(
        "ext_pwr_avail", "GPU", "world", "EXT_PWR_AVAIL:1",
    ),
}
