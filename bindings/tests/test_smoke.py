"""Smoke test del binding PyO3, desde el lado Python.

Es el espejo del test de Rust `cold_and_dark_to_battery_on_through_the_api_alone`
(core-rs/src/api.rs), conducido enteramente a través de la extensión `a320_sim`:
demuestra que instanciar, actuar controles, avanzar el tiempo y leer buses cruzan
el FFI correctamente, y que los errores del core afloran como excepciones Python.

Runnable de dos formas:
  - directo, sin dependencias:  python bindings/tests/test_smoke.py
  - bajo pytest:                pytest bindings/tests/
"""

import a320_sim

DC_BAT = "ELEC_DC_BAT_BUS_IS_POWERED"
AC_1 = "ELEC_AC_1_BUS_IS_POWERED"
BAT_1 = "OVHD_ELEC_BAT_1_PB_IS_AUTO"
BAT_2 = "OVHD_ELEC_BAT_2_PB_IS_AUTO"


def test_cold_and_dark_to_battery_on():
    """Cold & dark -> baterías ON -> el DC BAT bus cobra vida (sin AC)."""
    sim = a320_sim.Sim()

    # Cold & dark: toda la red sin alimentar.
    sim.step(1000)
    s = sim.get([DC_BAT, AC_1])
    assert s[DC_BAT] == 0.0, "DC BAT off en cold & dark"
    assert s[AC_1] == 0.0, "AC 1 off en cold & dark"

    # Baterías ON solo por la API (int 1; bool True también valdría).
    sim.set(BAT_1, 1)
    sim.set(BAT_2, 1)
    sim.run(2.0, 5.0)  # settling ~2 s a 5 Hz

    s = sim.get([DC_BAT, AC_1])
    assert s[DC_BAT] == 1.0, "DC BAT ON con baterías"
    assert s[AC_1] == 0.0, "AC 1 sigue off (sin fuente AC)"


def test_unknown_control_raises():
    """Un nombre con typo lanza UnknownControlError con mensaje útil."""
    sim = a320_sim.Sim()
    try:
        sim.set("OVHD_ELEC_BAT_1_PB_IS_ATUO", 1)
    except a320_sim.UnknownControlError as exc:
        assert "unknown control" in str(exc)
        assert isinstance(exc, a320_sim.SimError)
    else:
        raise AssertionError("se esperaba UnknownControlError")


def test_bad_value_raises():
    """Un valor no finito lanza BadValueError con mensaje útil."""
    sim = a320_sim.Sim()
    try:
        sim.set(BAT_1, float("nan"))
    except a320_sim.BadValueError as exc:
        assert "finite" in str(exc)
        assert isinstance(exc, a320_sim.SimError)
    else:
        raise AssertionError("se esperaba BadValueError")


def test_get_unknown_variable_raises():
    """get de una variable desconocida también es un error explícito."""
    sim = a320_sim.Sim()
    try:
        sim.get(["NO SUCH VAR"])
    except a320_sim.UnknownControlError:
        pass
    else:
        raise AssertionError("se esperaba UnknownControlError")


def test_discovery_and_snapshot():
    """list_variables y snapshot exponen el registro por el FFI."""
    sim = a320_sim.Sim()
    sim.step(200)

    variables = sim.list_variables()
    assert isinstance(variables, list)
    assert DC_BAT in variables

    snap = sim.snapshot()
    assert isinstance(snap, dict)
    assert len(snap) == len(variables), "snapshot cubre todas las variables"
    assert DC_BAT in snap


def test_list_controls_exposes_curated_catalog():
    """list_controls devuelve el catálogo curado como lista de dicts triviales."""
    sim = a320_sim.Sim()

    controls = sim.list_controls()
    assert isinstance(controls, list)
    assert len(controls) > 0

    by_name = {c["name"]: c for c in controls}
    # Los controles del panel eléctrico de Fase 1 deben estar todos.
    for name in ("bat_1", "bat_2", "ext_pwr", "apu_gen", "bus_tie", "gen_1", "gen_2"):
        assert name in by_name, f"falta el control '{name}'"

    bat_1 = by_name["bat_1"]
    # Cada entrada trae los metadatos del esquema, todos como str por el FFI.
    expected_keys = {
        "name", "lvar", "kind", "valid_values", "description", "group", "domain",
    }
    assert set(bat_1) == expected_keys
    assert all(isinstance(v, str) for v in bat_1.values())
    assert bat_1["lvar"] == "OVHD_ELEC_BAT_1_PB_IS_AUTO"
    assert bat_1["kind"] == "bool"
    assert bat_1["group"] == "ELEC"
    assert bat_1["domain"] == "cockpit"

    # El fake de mundo (GPU enchufado) se distingue por dominio.
    assert by_name["ext_pwr_avail"]["domain"] == "world"


def test_list_failures_exposes_curated_catalog():
    """list_failures devuelve el catálogo de fallos como dicts triviales."""
    sim = a320_sim.Sim()

    failures = sim.list_failures()
    assert isinstance(failures, list)
    assert len(failures) > 0

    by_id = {f["id"]: f for f in failures}
    for fid in ("elec.tr.1", "elec.gen.1", "elec.apu_gen.1", "elec.bus.ac.1"):
        assert fid in by_id, f"falta el fallo '{fid}'"

    tr_1 = by_id["elec.tr.1"]
    assert set(tr_1) == {"id", "ata", "description", "group"}
    # Todo str por el FFI, incluido el id numérico ATA.
    assert all(isinstance(v, str) for v in tr_1.values())
    assert tr_1["ata"] == "24000"
    assert tr_1["group"] == "ELEC"


def test_inject_and_clear_failure():
    """Inyectar un fallo por id cambia el avión; limpiarlo lo revierte."""
    sim = a320_sim.Sim()
    tr_1_normal = "ELEC_TR_1_POTENTIAL_NORMAL"

    # Red AC viva vía ext pwr (el TR 1 necesita AC para dar potencial normal).
    sim.set(BAT_1, 1)
    sim.set(BAT_2, 1)
    sim.set("bus_tie", 1)
    sim.set("ext_pwr_avail", 1)
    sim.set("ext_pwr", 1)
    sim.run(2.0, 5.0)
    assert sim.get([tr_1_normal])[tr_1_normal] == 1.0, "precondición: TR 1 normal"
    assert sim.active_failures() == []

    sim.inject_failure("elec.tr.1")
    sim.run(2.0, 5.0)
    assert sim.active_failures() == ["elec.tr.1"]
    assert sim.get([tr_1_normal])[tr_1_normal] == 0.0, "TR 1 fallado"

    sim.clear_failure("elec.tr.1")
    sim.run(2.0, 5.0)
    assert sim.active_failures() == []
    assert sim.get([tr_1_normal])[tr_1_normal] == 1.0, "TR 1 recuperado"


def test_unknown_failure_raises():
    """Un id de fallo desconocido lanza UnknownFailureError, no un panic."""
    sim = a320_sim.Sim()
    try:
        sim.inject_failure("elec.tr.99")
    except a320_sim.UnknownFailureError as exc:
        assert "unknown failure" in str(exc)
        assert "list_failures" in str(exc)
        assert isinstance(exc, a320_sim.SimError)
    else:
        raise AssertionError("se esperaba UnknownFailureError")


def test_environment_and_sim_time():
    """set_environment se refleja en las simvars; sim_time avanza."""
    sim = a320_sim.Sim()
    sim.set_environment(1000.0, 0.0, 5.0, 1013.25)
    sim.step(1000)

    s = sim.get(["SIM ON GROUND", "PRESSURE ALTITUDE"])
    assert s["SIM ON GROUND"] == 1.0
    assert abs(s["PRESSURE ALTITUDE"] - 1000.0) < 1e-6
    assert sim.sim_time() > 0.0


if __name__ == "__main__":
    tests = sorted(
        (name, fn)
        for name, fn in globals().items()
        if name.startswith("test_") and callable(fn)
    )
    for name, fn in tests:
        fn()
        print(f"ok  {name}")
    print(f"\n{len(tests)} smoke tests passed.")
