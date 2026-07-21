//! Test de integración del vertical slice hidráulico **sin motores** (issue #55).
//!
//! Es el primer escenario de Fase 4: con la red AC alimentada por ext pwr, las
//! bombas eléctricas y el PTU bastan para presurizar los tres circuitos — las
//! EDP (bombas de motor) llegan con el arranque de motores (slice 4).
//!
//! ## Semántica del vendor validada aquí (rutas en `core-rs/vendor/aircraft`)
//!
//! - **Bomba amarilla** (`A320YellowElectricPumpController::update`,
//!   `a320_systems/src/hydraulic/mod.rs:3310-3312`): bombea si el pulsador está
//!   en **ON** (AUTO/ON invertido: LVAR `IS_AUTO` = 0) y su alimentación (AC
//!   GND/FLT SVC, `:1597`) está viva. En AUTO solo arranca por operación de
//!   cargo door. Es el único circuito presurizable "directo" en tierra.
//! - **Bomba azul** (`A320BlueElectricPumpController::update`, `:3134-3142`):
//!   en AUTO solo bombea con morro descomprimido (vuelo), un motor en marcha o
//!   el **override** del panel de mantenimiento — cuyo LVAR se llama
//!   `HYD_EPUMPY_OVRD` pese a ser el BLUE PUMP OVRD (`:4506`, consumido solo
//!   por el controlador azul en `:3139`).
//! - **PTU** (`A320PowerTransferUnitController::update`, `:3491-3497`): en
//!   tierra con ambos starters inactivos (`GENERAL ENG STARTER ACTIVE:{1,2}` a
//!   0), la rama `!eng_1_master_on && !eng_2_master_on` habilita el PTU con el
//!   pulsador en AUTO — transfiere del amarillo al verde sin más condiciones.
//! - **Fuga de reservorio** (`Reservoir::update_leak_failure`,
//!   `fbw-common/.../hydraulic/mod.rs:2336-2343`): drena 0.1 gal/s y el fluido
//!   **no vuelve** al limpiar el fallo — como en el avión real, la fuga se
//!   detiene pero el fluido perdido está perdido, así que la presión no se
//!   recupera sola: el test fija ese comportamiento real, y la vía de "gestión"
//!   es apagar la bomba (el LO PR se retira porque ya no hay fuente comandada).

use a320_sim_core::api::Sim;
use a320_sim_core::ecam::{EcamSource, Severity, Warning};
use a320_sim_core::failures::FailureGroup;

// --- Controles (nombres amigables del catálogo) ------------------------------
const EPUMP_YELLOW: &str = "hyd_epump_yellow"; // 1 = AUTO, 0 = ON (invertido)
const EPUMP_BLUE: &str = "hyd_epump_blue"; // 1 = AUTO, 0 = OFF
const EPUMP_BLUE_OVRD: &str = "hyd_epump_blue_ovrd"; // momentary (toggle por flanco)
const PTU: &str = "hyd_ptu"; // 1 = AUTO, 0 = OFF

// --- Outputs (escriben los sistemas) -----------------------------------------
const GREEN_PRESSURE: &str = "HYD_GREEN_SYSTEM_1_SECTION_PRESSURE";
const BLUE_PRESSURE: &str = "HYD_BLUE_SYSTEM_1_SECTION_PRESSURE";
const YELLOW_PRESSURE: &str = "HYD_YELLOW_SYSTEM_1_SECTION_PRESSURE";
const GREEN_RESERVOIR: &str = "HYD_GREEN_RESERVOIR_LEVEL";
const BLUE_RESERVOIR: &str = "HYD_BLUE_RESERVOIR_LEVEL";
const YELLOW_RESERVOIR: &str = "HYD_YELLOW_RESERVOIR_LEVEL";
const PTU_MEMO: &str = "HYD_PTU_ON_ECAM_MEMO";
const BLUE_OVRD_IS_ON: &str = "OVHD_HYD_EPUMPY_OVRD_IS_ON";

/// Presión "circuito presurizado nominal" (bomba eléctrica ~3000 psi).
const NOMINAL_PSI: std::ops::RangeInclusive<f64> = 2800.0..=3100.0;
/// Umbral LO PR (el del low press switch del vendor).
const LO_PR_PSI: f64 = 1450.0;
/// Presión "circuito muerto" (ruido numérico aparte).
const DEPRESSURISED_PSI: f64 = 100.0;

fn pressure(sim: &Sim, var: &str) -> f64 {
    sim.get(&[var]).unwrap()[var]
}

fn find<'a>(ecam: &'a [Warning], id: &str) -> Option<&'a Warning> {
    ecam.iter().find(|w| w.id == id)
}

/// Espera acotada: avanza en pasos de 1 s (5 Hz) hasta que `pred` se cumpla.
fn run_until(sim: &mut Sim, timeout_s: u32, what: &str, pred: impl Fn(&Sim) -> bool) {
    let mut elapsed = 0;
    while !pred(sim) {
        sim.run(1.0, 5.0);
        elapsed += 1;
        assert!(elapsed <= timeout_s, "timeout ({timeout_s} s): {what}");
    }
}

/// Red AC completa por ext pwr, con el panel hidráulico en reposo real: bomba
/// amarilla aparcada en AUTO (su LVAR AUTO/ON invertido lee 0 = ON sin seeding,
/// D-007) y el resto en su default OFF.
fn powered_network() -> Sim {
    let mut sim = Sim::new();
    sim.set(EPUMP_YELLOW, 1.0).unwrap(); // AUTO = bomba parada en tierra
    sim.set("bat_1", 1.0).unwrap();
    sim.set("bat_2", 1.0).unwrap();
    sim.set("bus_tie", 1.0).unwrap();
    sim.set("ext_pwr_avail", 1.0).unwrap();
    sim.set("ext_pwr", 1.0).unwrap();
    sim.run(3.0, 5.0);
    sim
}

/// Red alimentada y circuito amarillo presurizado por su bomba eléctrica.
fn yellow_pressurised() -> Sim {
    let mut sim = powered_network();
    sim.set(EPUMP_YELLOW, 0.0).unwrap(); // ON (invertido)
    run_until(
        &mut sim,
        30,
        "el circuito amarillo no llegó a presión nominal",
        |s| pressure(s, YELLOW_PRESSURE) >= *NOMINAL_PSI.start(),
    );
    sim
}

// --- (1) Cold & dark ---------------------------------------------------------

#[test]
fn cold_and_dark_hydraulics_are_depressurised_with_a_silent_ecam() {
    let mut sim = Sim::new();
    sim.run(3.0, 5.0);

    for var in [GREEN_PRESSURE, BLUE_PRESSURE, YELLOW_PRESSURE] {
        let p = pressure(&sim, var);
        assert!(
            p < DEPRESSURISED_PSI,
            "cold & dark: {var} debería estar despresurizado, fue {p} psi"
        );
    }
    // Los reservorios están llenos (el fluido existe aunque nadie bombee).
    for var in [GREEN_RESERVOIR, BLUE_RESERVOIR, YELLOW_RESERVOIR] {
        let level = pressure(&sim, var);
        assert!(
            level > 0.5,
            "cold & dark: {var} debería tener fluido, fue {level} gal"
        );
    }
    // Y la ECAM no inventa un LO PR: sin fuente comandada, presión 0 no es
    // fallo (además de que en cold & dark la ECAM ni siquiera está alimentada).
    assert!(
        sim.read_ecam().is_empty(),
        "cold & dark: ECAM debería estar limpia, fue {:?}",
        sim.read_ecam()
    );
}

// --- (2) Bomba eléctrica amarilla en tierra ----------------------------------

#[test]
fn yellow_epump_auto_keeps_the_pump_stopped_and_on_pressurises_the_circuit() {
    let mut sim = powered_network();

    // En AUTO la bomba amarilla NO bombea en tierra (solo cargo door): la
    // presión sigue muerta aunque el AC esté vivo.
    let p = pressure(&sim, YELLOW_PRESSURE);
    assert!(
        p < DEPRESSURISED_PSI,
        "epump amarilla en AUTO en tierra: no debería bombear, presión {p} psi"
    );
    assert!(
        sim.read_ecam().is_empty(),
        "red sana con bomba en AUTO: ECAM limpia, fue {:?}",
        sim.read_ecam()
    );

    // ON (LVAR invertido: IS_AUTO = 0): la bomba presuriza a ~3000 psi.
    sim.set(EPUMP_YELLOW, 0.0).unwrap();
    run_until(
        &mut sim,
        30,
        "el circuito amarillo no llegó a presión nominal",
        |s| pressure(s, YELLOW_PRESSURE) >= *NOMINAL_PSI.start(),
    );
    let p = pressure(&sim, YELLOW_PRESSURE);
    assert!(
        NOMINAL_PSI.contains(&p),
        "epump amarilla ON: presión esperada en {NOMINAL_PSI:?} psi, fue {p}"
    );

    // Los otros circuitos siguen muertos: el PTU está en OFF (default D-007) y
    // la bomba azul sin override.
    for var in [GREEN_PRESSURE, BLUE_PRESSURE] {
        let p = pressure(&sim, var);
        assert!(
            p < DEPRESSURISED_PSI,
            "solo bombea la amarilla: {var} debería seguir muerto, fue {p} psi"
        );
    }

    // Y con la presión nominal alcanzada, la ECAM queda limpia (el transitorio
    // de arranque puede pasar por LO PR; el estado estable no).
    sim.run(2.0, 5.0);
    assert!(
        sim.read_ecam().is_empty(),
        "amarillo presurizado y sano: ECAM limpia, fue {:?}",
        sim.read_ecam()
    );
}

// --- (3) PTU: transferencia amarillo -> verde --------------------------------

#[test]
fn ptu_in_auto_transfers_yellow_pressure_into_green_and_raises_the_memo() {
    let mut sim = yellow_pressurised();
    assert!(
        pressure(&sim, GREEN_PRESSURE) < DEPRESSURISED_PSI,
        "precondición: verde muerto antes de habilitar el PTU"
    );

    // PTU en AUTO: en tierra con ambos starters inactivos el controlador lo
    // habilita (rama `!eng_1 && !eng_2`, hydraulic/mod.rs:3494) y transfiere
    // del amarillo presurizado al verde.
    sim.set(PTU, 1.0).unwrap();
    run_until(
        &mut sim,
        60,
        "el PTU no levantó el circuito verde por transferencia",
        |s| pressure(s, GREEN_PRESSURE) > LO_PR_PSI,
    );

    // El memo del PTU (lógica del vendor) está activo y la ECAM lo muestra.
    assert!(
        pressure(&sim, PTU_MEMO) != 0.0,
        "con el PTU transfiriendo, HYD_PTU_ON_ECAM_MEMO debería ser 1"
    );
    let ecam = sim.read_ecam();
    let memo = find(&ecam, "hyd.ptu.memo")
        .unwrap_or_else(|| panic!("se esperaba el memo HYD PTU, ECAM: {ecam:?}"));
    assert_eq!(memo.message, "HYD PTU");
    assert_eq!(memo.severity, Severity::Advisory);
    assert_eq!(memo.source, EcamSource::VendorFlag);

    // Con el verde por encima del umbral, no hay LO PR verde.
    assert!(
        find(&ecam, "hyd.g.lo_pr").is_none(),
        "verde presurizado por el PTU: no debería haber LO PR verde, ECAM: {ecam:?}"
    );
}

// --- (4) Bomba azul con override ---------------------------------------------

#[test]
fn blue_pump_override_runs_the_blue_epump_on_ground() {
    let mut sim = powered_network();

    // Bomba azul en AUTO: en tierra sin motores no bombea por sí sola.
    sim.set(EPUMP_BLUE, 1.0).unwrap();
    sim.run(3.0, 5.0);
    assert!(
        pressure(&sim, BLUE_PRESSURE) < DEPRESSURISED_PSI,
        "bomba azul en AUTO en tierra: no debería bombear sin override"
    );
    // El LGCIU ve el tren comprimido (weight-on-wheels): es la condición que
    // mantiene la bomba azul parada en tierra. Sin fakear la compresión de los
    // amortiguadores en el Environment, el avión se creería en vuelo.
    let s = sim.get(&["LGCIU_1_NOSE_GEAR_COMPRESSED"]).unwrap();
    assert_eq!(
        s["LGCIU_1_NOSE_GEAR_COMPRESSED"], 1.0,
        "el LGCIU 1 debería reportar el morro comprimido en tierra"
    );

    // Pulsación del override (momentary: conmuta en el flanco 0->1). El vendor
    // lo llama HYD_EPUMPY_OVRD pero es el BLUE PUMP OVRD (ver catálogo).
    sim.set(EPUMP_BLUE_OVRD, 1.0).unwrap();
    sim.step(200);
    sim.set(EPUMP_BLUE_OVRD, 0.0).unwrap(); // soltar, para poder re-pulsar
    sim.step(200);
    assert!(
        pressure(&sim, BLUE_OVRD_IS_ON) != 0.0,
        "tras pulsar, el estado del override (IS_ON, lo escribe FBW) debería ser 1"
    );

    run_until(
        &mut sim,
        30,
        "el circuito azul no llegó a presión nominal",
        |s| pressure(s, BLUE_PRESSURE) >= *NOMINAL_PSI.start(),
    );
    let p = pressure(&sim, BLUE_PRESSURE);
    assert!(
        NOMINAL_PSI.contains(&p),
        "bomba azul con override: presión esperada en {NOMINAL_PSI:?} psi, fue {p}"
    );
}

// --- (5) Fallo: fuga del reservorio amarillo ---------------------------------

#[test]
fn yellow_reservoir_leak_collapses_pressure_and_raises_lo_pr() {
    let mut sim = yellow_pressurised();

    sim.inject_failure("hyd.reservoir_leak.yellow").unwrap();
    assert_eq!(sim.active_failures(), vec!["hyd.reservoir_leak.yellow"]);

    // La fuga drena 0.1 gal/s (~3.8 gal de reservorio): la bomba acaba
    // cavitando y la presión se desploma por debajo del umbral LO PR.
    run_until(
        &mut sim,
        120,
        "la fuga del reservorio no llegó a tirar la presión amarilla",
        |s| pressure(s, YELLOW_PRESSURE) < LO_PR_PSI,
    );

    // La ECAM lo cuenta: LO PR amarillo (regla nuestra, con la bomba todavía
    // comandada — eso es lo que separa "averiado" de "apagado").
    let ecam = sim.read_ecam();
    let lo_pr = find(&ecam, "hyd.y.lo_pr")
        .unwrap_or_else(|| panic!("se esperaba HYD Y SYS LO PR, ECAM: {ecam:?}"));
    assert_eq!(lo_pr.message, "HYD Y SYS LO PR");
    assert_eq!(lo_pr.severity, Severity::Caution);
    assert_eq!(lo_pr.system, FailureGroup::Hyd);
    assert_eq!(lo_pr.source, EcamSource::Derived);

    // --- Limpiar el fallo detiene la fuga, pero el fluido NO vuelve ----------
    // (Reservoir::update_leak_failure solo resta; no hay camino de retorno del
    // fluido perdido). Como en el avión real: se fija el comportamiento real.
    sim.clear_failure("hyd.reservoir_leak.yellow").unwrap();
    assert!(sim.active_failures().is_empty());
    let level_after_clear = pressure(&sim, YELLOW_RESERVOIR);
    sim.run(5.0, 5.0);
    let level_later = pressure(&sim, YELLOW_RESERVOIR);
    assert!(
        level_later >= level_after_clear - 0.05,
        "fuga limpiada: el nivel no debería seguir cayendo ({level_after_clear} -> {level_later} gal)"
    );

    // La gestión del fallo es apagar la bomba (QRH-style): sin fuente comandada
    // el LO PR se retira — el circuito ya no está "averiado", está apagado.
    sim.set(EPUMP_YELLOW, 1.0).unwrap(); // AUTO = bomba parada
    sim.run(2.0, 5.0);
    let ecam = sim.read_ecam();
    assert!(
        find(&ecam, "hyd.y.lo_pr").is_none(),
        "bomba apagada: el LO PR amarillo debería retirarse, ECAM: {ecam:?}"
    );
}

// --- (6) Catálogo: espejo de los tests de Fase 2 -----------------------------

#[test]
fn hydraulic_failure_catalog_is_unique_in_range_and_round_trips() {
    let mut sim = Sim::new();
    let hyd: Vec<_> = sim
        .list_failures()
        .into_iter()
        .filter(|f| f.group == FailureGroup::Hyd)
        .collect();

    // Las 13 entradas ATA29 de FBW (a320_systems_wasm/src/lib.rs:164-200).
    assert_eq!(hyd.len(), 13, "se esperaban los 13 fallos ATA29");

    for (i, a) in hyd.iter().enumerate() {
        assert!(
            (29_000..30_000).contains(&a.ata),
            "'{}' tiene un ATA fuera del rango hidráulico: {}",
            a.id,
            a.ata
        );
        assert!(a.id.starts_with("hyd."), "id sin prefijo hyd.: {}", a.id);
        for b in &hyd[i + 1..] {
            assert_ne!(a.id, b.id, "id duplicado: {}", a.id);
            assert_ne!(a.ata, b.ata, "ATA duplicado: {}", a.ata);
        }
    }

    // Round-trip inyectar/limpiar por la API para cada id (sin ticks: el
    // contrato de active_failures es inmediato).
    for def in &hyd {
        sim.inject_failure(def.id).unwrap();
        assert!(
            sim.active_failures().contains(&def.id),
            "'{}' no aparece en active_failures tras inyectarlo",
            def.id
        );
        sim.clear_failure(def.id).unwrap();
        assert!(
            !sim.active_failures().contains(&def.id),
            "'{}' sigue activo tras limpiarlo",
            def.id
        );
    }
}
