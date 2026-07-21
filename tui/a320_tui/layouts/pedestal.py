"""PEDESTAL zone: three columns per the mockup (`tui/docs/pedestal-layout.md`).

MCDU/RMP/ACP are instances 1 and 2 of the pedestal templates (instance 3
of each lives on the overhead). The surfaces section splits across the
mockup's boxes: speed brake left, thrust quadrant with the trim wheels
center, rudder trim center, flaps right.
"""

from __future__ import annotations

from a320_tui.layouts import AutoSection, Section, ZoneSpec

_LEFT = (
    AutoSection("MCDU 1", "pedestal", "mcdu", prefix="MCDU_1"),
    AutoSection("RMP 1", "pedestal", "rmp", prefix="RMP_1"),
    AutoSection("ACP 1", "pedestal", "acp", prefix="ACP_1", per_row=3),
    AutoSection("LT", "pedestal", "lighting", per_row=3),
    AutoSection("WX RADAR", "pedestal", "wx_radar"),
    Section("SPEED BRAKE", (("SPD_BRK_LEVER", "GND_SPLRS_ARMED"),)),
    AutoSection("COCKPIT DOOR", "pedestal", "cockpit_door"),
    Section("DATA LOADER", (("DATA_LOADER_ACCESS",),)),
)

_CENTER = (
    AutoSection("SWITCHING", "pedestal", "switching"),
    AutoSection("ECAM CP", "pedestal", "ecam_cp", per_row=6),
    Section(
        "THRUST QUADRANT",
        (
            (
                "PITCH_TRIM_WHEEL_L",
                "THR_LEVER_1",
                "THR_LEVER_2",
                "PITCH_TRIM_WHEEL_R",
            ),
            ("REV_LATCH_1", "REV_LATCH_2"),
            ("ATHR_DISC_1", "ATHR_DISC_2"),
        ),
    ),
    AutoSection("ENG", "pedestal", "eng", per_row=3),
    Section(
        "RUD TRIM",
        (("RUDDER_TRIM_IND", "RUDDER_TRIM_KNOB", "RUDDER_TRIM_RESET"),),
    ),
    AutoSection("PARKING BRK", "pedestal", "park_brake"),
    AutoSection("GEAR GRVTY EXTN", "pedestal", "gear_gravity"),
    Section("HANDSET", (("HANDSET",),)),
)

_RIGHT = (
    AutoSection("MCDU 2", "pedestal", "mcdu", prefix="MCDU_2"),
    AutoSection("RMP 2", "pedestal", "rmp", prefix="RMP_2"),
    AutoSection("ACP 2", "pedestal", "acp", prefix="ACP_2", per_row=3),
    AutoSection("AIDS · DFDR", "pedestal", "aids_dfdr", per_row=2),
    AutoSection("ATC / TCAS", "pedestal", "atc_tcas"),
    Section("FLAPS", (("FLAPS_LEVER",),)),
    Section("PRINTER", (("PRINTER",),)),
)

PEDESTAL_ZONE = ZoneSpec("PEDESTAL", (_LEFT, _CENTER, _RIGHT))
