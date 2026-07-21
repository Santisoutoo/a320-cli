"""Vendored A320 cockpit controls model (single source of truth: the YAML)."""

from a320_tui.model.loader import CockpitModel, ControlDef, load_model

__all__ = ["CockpitModel", "ControlDef", "load_model"]
