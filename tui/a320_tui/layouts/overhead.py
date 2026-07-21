"""OVERHEAD zone: aft + forward panels, three columns per the mockup
(`tui/docs/overhead-layout.md`). The aft sections sit on top of their
column, like the physical panel behind the forward one.

The ELEC section is hand-transcribed: it is the wired 35VU and its
geometry is load-bearing (voltmeters between the BAT korrys, the painted
bus mimic, the sources row). RMP 3 / ACP 3 are instances of the pedestal
templates placed here, which is what the YAML's ``rmp3_acp3`` note means.
"""

from __future__ import annotations

from a320_tui.layouts import AutoSection, Section, ZoneSpec

_LEFT = (
    AutoSection("AFT · CIRCUIT BREAKERS", "overhead_aft", "circuit_breakers"),
    AutoSection("CKPT DOOR", "overhead_fwd", "cockpit_door"),
    AutoSection("ADIRS", "overhead_fwd", "adirs", per_row=3),
    AutoSection("FLT CTL", "overhead_fwd", "f_ctl_left", per_row=3),
    AutoSection("EVAC", "overhead_fwd", "evac", per_row=3),
    AutoSection("EMER ELEC PWR", "overhead_fwd", "emer_elec_pwr"),
    AutoSection("GPWS", "overhead_fwd", "gpws", per_row=3),
    AutoSection("RCDR", "overhead_fwd", "rcdr", per_row=3),
    AutoSection("OXYGEN", "overhead_fwd", "oxygen", per_row=3),
    AutoSection("CALLS", "overhead_fwd", "calls"),
    Section(
        "RAIN RPLNT · WIPER",
        (("RAIN_RPLNT_CAPT", "WIPER_CAPT"),),
    ),
)

_CENTER = (
    AutoSection("AFT · MAINTENANCE", "overhead_aft", "maintenance"),
    AutoSection("FIRE", "overhead_fwd", "fire"),
    AutoSection("HYD", "overhead_fwd", "hyd", per_row=3),
    AutoSection("FUEL", "overhead_fwd", "fuel"),
    Section(
        "ELEC",
        (
            ("COMMERCIAL", "BAT_DISPLAY", "AC_ESS_FEED"),
            ("BAT_1", "BAT_2"),
            ("GALY_CAB", "IDG_1", "GEN_1", "APU_GEN"),
            ("BUS_TIE", "EXT_PWR", "GEN_2", "IDG_2"),
            ("mimic:bus",),
        ),
    ),
    AutoSection("AIR COND", "overhead_fwd", "air_cond"),
    AutoSection("ANTI ICE", "overhead_fwd", "anti_ice"),
    AutoSection("CABIN PRESS", "overhead_fwd", "cabin_press"),
    AutoSection("EXT LT", "overhead_fwd", "ext_lt"),
    AutoSection("APU", "overhead_fwd", "apu", per_row=2),
    AutoSection("SIGNS", "overhead_fwd", "signs", per_row=3),
    AutoSection("INT LT", "overhead_fwd", "int_lt", per_row=3),
)

_RIGHT = (
    Section(
        "AFT · ELT · LIGHTS",
        (
            ("ELT",),
            ("READING_LT_CAPT", "READING_LT_FO", "DOME_LT"),
        ),
    ),
    AutoSection("ACP 3", "pedestal", "acp", prefix="ACP_3", per_row=3),
    AutoSection("RMP 3", "pedestal", "rmp", prefix="RMP_3"),
    AutoSection("FLT CTL", "overhead_fwd", "f_ctl_right"),
    Section(
        "CARGO HEAT",
        (
            ("CARGO_FWD_ISOL_VALVE", "CARGO_AFT_ISOL_VALVE"),
            ("CARGO_HEAT_FWD_TEMP", "CARGO_HOT_AIR", "CARGO_HEAT_AFT_TEMP"),
        ),
    ),
    Section(
        "CARGO SMOKE",
        (
            ("CARGO_SMOKE_FWD", "CARGO_SMOKE_AFT"),
            ("CARGO_SMOKE_DISCH",),
        ),
    ),
    AutoSection("VENTILATION", "overhead_fwd", "ventilation", per_row=3),
    AutoSection("ENG", "overhead_fwd", "eng_man_start", per_row=2),
    Section(
        "WIPER · RAIN RPLNT",
        (("WIPER_FO", "RAIN_RPLNT_FO"),),
    ),
)

OVERHEAD_ZONE = ZoneSpec("OVERHEAD", (_LEFT, _CENTER, _RIGHT))
