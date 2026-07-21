"""MAIN PANEL zone, approximate scheme (`tui/docs/cockpit-layout.md`):
PFD/ND CAPT | ISIS·clock·DDRMI | ECAM + gear/brakes | PFD/ND F/O, plus
an OTHER column for the sidesticks and tillers, which the mockup leaves
outside the four zones.
"""

from __future__ import annotations

from a320_tui.layouts import AutoSection, Section, ZoneSpec

_CAPT = (
    Section(
        "PFD · ND CAPT",
        (
            ("PFD_CAPT", "ND_CAPT"),
            ("PFD_BRT_CAPT", "ND_BRT_CAPT"),
            ("PFD_ND_XFR_CAPT", "TERR_ON_ND_CAPT"),
        ),
    ),
)

_STANDBY = (
    AutoSection("ISIS", "main_panel", "isis", per_row=3),
    AutoSection("CLOCK", "main_panel", "clock", per_row=2),
    AutoSection("DDRMI", "main_panel", "ddrmi", per_row=2),
)

_CENTER = (
    Section("ECAM", (("EWD",), ("SD",))),
    AutoSection("LDG GEAR", "main_panel", "landing_gear", per_row=1),
    AutoSection("BRAKES", "main_panel", "brakes", per_row=3),
)

_FO = (
    Section(
        "PFD · ND F/O",
        (
            ("PFD_FO", "ND_FO"),
            ("PFD_BRT_FO", "ND_BRT_FO"),
            ("PFD_ND_XFR_FO", "TERR_ON_ND_FO"),
        ),
    ),
)

_OTHER = (
    AutoSection(
        "SIDESTICK CAPT", "other", "sidestick",
        prefix="SIDESTICK_CAPT", per_row=1,
    ),
    AutoSection(
        "SIDESTICK F/O", "other", "sidestick",
        prefix="SIDESTICK_FO", per_row=1,
    ),
    AutoSection("TILLER CAPT", "other", "tiller", prefix="TILLER_CAPT", per_row=1),
    AutoSection("TILLER F/O", "other", "tiller", prefix="TILLER_FO", per_row=1),
)

MAIN_PANEL_ZONE = ZoneSpec(
    "MAIN PANEL", (_CAPT, _STANDBY, _CENTER, _FO, _OTHER)
)
