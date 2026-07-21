from textual.widget import Widget

from a320_tui.controller import ControlView
from a320_tui.widgets.annunciator import GaugeBox, LightAnnunciator
from a320_tui.widgets.base import CockpitControlWidget, LocalKorry
from a320_tui.widgets.elec_synoptic import ElecSynoptic
from a320_tui.widgets.ewd import EwdPanel
from a320_tui.widgets.fire_button import FirePushButton
from a320_tui.widgets.guarded_button import GuardedButton
from a320_tui.widgets.key_group import KeyGroup
from a320_tui.widgets.korry_button import KorryButton
from a320_tui.widgets.lever import Lever
from a320_tui.widgets.overhead_panel import OverheadPanel
from a320_tui.widgets.rotary import Knob, KnobPushPull, RotarySelector
from a320_tui.widgets.status_bar import StatusBar
from a320_tui.widgets.switch import ToggleSwitch
from a320_tui.widgets.wheel import TrimWheel

__all__ = [
    "CockpitControlWidget",
    "ElecSynoptic",
    "EwdPanel",
    "FirePushButton",
    "GaugeBox",
    "GuardedButton",
    "KeyGroup",
    "Knob",
    "KnobPushPull",
    "KorryButton",
    "Lever",
    "LightAnnunciator",
    "LocalKorry",
    "OverheadPanel",
    "RotarySelector",
    "StatusBar",
    "ToggleSwitch",
    "TrimWheel",
    "widget_for",
]


def widget_for(view: ControlView) -> Widget:
    """Instantiate the right local widget for a model control's view.

    Composites (``keys``) win over the base type: an MCDU keyboard is one
    KeyGroup, not 30 korrys. Wired controls never come through here — they
    keep their ``KorryButton`` over ``SimState``.
    """
    if view.keys:
        return KeyGroup(view)
    if view.ctype in ("pb", "pb_mom"):
        return LocalKorry(view)
    if view.ctype == "pb_guard":
        return GuardedButton(view)
    if view.ctype == "fire_pb":
        return FirePushButton(view)
    if view.ctype == "sel" or (view.ctype == "knob" and view.positions):
        return RotarySelector(view)
    if view.ctype == "knob":
        return Knob(view)
    if view.ctype == "knob_pp":
        return KnobPushPull(view)
    if view.ctype == "sw":
        return ToggleSwitch(view)
    if view.ctype == "lever":
        return Lever(view)
    if view.ctype == "wheel":
        return TrimWheel(view)
    if view.ctype == "light":
        return LightAnnunciator(view)
    return GaugeBox(view)
