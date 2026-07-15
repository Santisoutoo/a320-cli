# a320-sim — PyO3 bindings

Python extension exposing the headless A320 systems core (`a320-sim-core`, which
vendors and links [FlyByWire](https://github.com/flybywiresim/aircraft)'s Rust
systems crates) as a synchronous `Sim` class.

Only trivial types cross the FFI boundary — `f64` / `bool` / `str` / list / dict.
No FBW types leak into Python. Typed core errors (`ApiError`) surface as Python
exceptions, never panics.

## Requirements

Building this package needs **both** toolchains, because it compiles the Rust
core (and the vendored FBW monorepo) into a Python extension:

- **Rust 1.93.0** — auto-installs from `rust-toolchain.toml` on first build.
  On Windows this is the MSVC target (Visual Studio C++ Build Tools).
- **Python ≥ 3.9** with `pip`.
- The **FBW submodule** must be checked out first (`core-rs/vendor/aircraft`),
  e.g. via `scripts\bootstrap-vendor.ps1` from the repo root.

## Build (editable install)

From this `bindings/` directory, in a clean virtual environment:

```powershell
python -m venv .venv
.\.venv\Scripts\Activate.ps1      # Linux/macOS: source .venv/bin/activate
pip install -e .
```

(`pip` pulls maturin in automatically via build isolation, per `pyproject.toml`;
installing it yourself is only needed for `maturin develop`.)

The first build compiles the whole FBW dependency graph and takes several
minutes. After it finishes:

```python
import a320_sim

sim = a320_sim.Sim()                 # cold & dark on the apron
sim.set("OVHD_ELEC_BAT_1_PB_IS_AUTO", 1)
sim.set("OVHD_ELEC_BAT_2_PB_IS_AUTO", 1)
sim.run(2.0, 5.0)                    # settle ~2 s at 5 Hz
print(sim.get(["ELEC_DC_BAT_BUS_IS_POWERED"]))   # -> {'ELEC_DC_BAT_BUS_IS_POWERED': 1.0}
```

`maturin develop` (from an activated venv) is the fast iteration alternative to
`pip install -e .` and produces the same importable module.

## API

`Sim()` mirrors the core `api::Sim` contract 1:1:

| Method | Signature | Notes |
|---|---|---|
| `set` | `set(control: str, value: float) -> None` | `True`/`1` accepted; raises `UnknownControlError` / `BadValueError` |
| `get` | `get(vars: list[str]) -> dict[str, float]` | raises `UnknownControlError` on an unknown name |
| `step` | `step(dt_ms: int) -> None` | one tick |
| `run` | `run(seconds: float, rate: float) -> None` | ticks at `rate` Hz |
| `set_environment` | `set_environment(altitude_ft, indicated_airspeed_kt, oat_celsius, qnh_hpa) -> None` | the outside world |
| `snapshot` | `snapshot() -> dict[str, float]` | every known variable |
| `list_variables` | `list_variables() -> list[str]` | discovery |
| `sim_time` | `sim_time() -> float` | monotonic seconds |

Exceptions: `SimError` (base), `UnknownControlError`, `BadValueError`.

`read_ecam()` and failure injection (Phase 2) and `list_controls()` (issue #12)
are added when they exist in the core; they are intentionally not stubbed here.

## Tests

```powershell
cargo test                                   # native: compiles the crate + core
python tests\test_smoke.py                   # Python smoke test (no pytest needed)
```

## License

GPL-3.0-or-later — inherited from the linked FlyByWire crates.
