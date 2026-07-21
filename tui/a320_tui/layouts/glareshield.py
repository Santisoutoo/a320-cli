"""GLARESHIELD zone, approximate scheme (`tui/docs/cockpit-layout.md`):
warnings CAPT | EFIS CAPT | FCU | EFIS F/O | warnings F/O (+ AUTOLAND,
which exists only once, on the F/O side).
"""

from __future__ import annotations

from a320_tui.layouts import AutoSection, Section, ZoneSpec

GLARESHIELD_ZONE = ZoneSpec(
    "GLARESHIELD",
    (
        (
            AutoSection(
                "WARN · CAPT", "glareshield", "warnings",
                prefix="WARN_CAPT", per_row=1,
            ),
        ),
        (
            AutoSection(
                "EFIS CAPT", "glareshield", "efis",
                prefix="EFIS_CAPT", per_row=3,
            ),
        ),
        (AutoSection("FCU", "glareshield", "fcu"),),
        (
            AutoSection(
                "EFIS F/O", "glareshield", "efis",
                prefix="EFIS_FO", per_row=3,
            ),
        ),
        (
            AutoSection(
                "WARN · F/O", "glareshield", "warnings",
                prefix="WARN_FO", per_row=1,
            ),
            Section("AUTOLAND", (("AUTOLAND",),)),
        ),
    ),
)
