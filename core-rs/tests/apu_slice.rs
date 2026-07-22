//! Test de integración del vertical slice del APU (issue #56, Fase 4 slice 2).
//!
//! El APU deja de operarse por LVAR crudo: MASTER, START y BLEED son controles
//! del catálogo y AVAIL se observa por `read_ecam`. El escenario es el mismo que
//! el de `generator_caution.rs` (arranque con baterías, sin ext pwr), pero aquí
//! el objeto del test es el propio APU, no su generador.
//!
//! ## Semántica del vendor validada aquí (rutas en `core-rs/vendor/aircraft`)
//!
//! - **Arranque solo con baterías**: el motor de arranque del APU cuelga de los
//!   dos contactores de batería (`a320_systems/src/electrical/direct_current.rs:206`).
//!   No hace falta ext pwr ni red AC.
//! - **Fuel**: el ECB vigila un `FuelPressureSwitch`
//!   (`fbw-common/.../systems/src/apu/mod.rs:78-93`) alimentado desde
//!   `left_inner_tank_has_fuel_remaining()` (`a320_systems/src/lib.rs:158-172`,
//!   `a320_systems/src/fuel/mod.rs:134-137`); si el APU gira sin presión de
//!   combustible, levanta `ApuFault::FuelLowPressure`
//!   (`electronic_control_box.rs:224-230`). Desde el slice 3 (#57) el runtime
//!   siembra la carga por defecto (~6 400 kg, `runtime::FUEL_SEED_GALLONS`) y
//!   el tanque left main del que bebe el APU tiene fuel real: la muleta
//!   `UNLIMITED FUEL` ya no hace falta (su retirada la vigila
//!   `tests/fuel_slice.rs`).
//! - **AVAIL**: el ECB declara el APU disponible con N>95% sostenido 2 s, o
//!   N>99.5% (`apu/electronic_control_box.rs:328-337`); el APS3200 tarda ~62 s
//!   medidos.
//! - **Bleed**: el ECB abre la válvula de bleed con MASTER ON, N>95% y el
//!   pulsador APU BLEED en ON (`electronic_control_box.rs:469-482`); el estado
//!   de la válvula lo escribe el neumático en `APU_BLEED_AIR_VALVE_OPEN`
//!   (`a320_systems/src/pneumatic.rs:136-137`, write en `:435-436`).
//! - **ECAM con solo baterías**: el gate de alimentación de `read_ecam` mira
//!   AC ESS **o** DC ESS, y el DC ESS está vivo con las baterías dentro (lo
//!   afirma `generator_caution.rs` tras perder toda la red AC) — no hace falta
//!   montar la red AC para leer el memo.

use a320_sim_core::api::Sim;
use a320_sim_core::ecam::{EcamSource, Severity, Warning};

// --- Controles (nombres amigables del catálogo) ------------------------------
const APU_MASTER: &str = "apu_master";
const APU_START: &str = "apu_start";
const APU_BLEED: &str = "apu_bleed";

// --- Outputs (escriben los sistemas) -----------------------------------------
const APU_AVAILABLE: &str = "OVHD_APU_START_PB_IS_AVAILABLE";
const BLEED_VALVE_OPEN: &str = "APU_BLEED_AIR_VALVE_OPEN";

/// Cota superior del arranque, en segundos de simulación: el APS3200 llega a
/// available a los ~62 s (ver doc del módulo); 90 s da margen sin colgar el CI.
const APU_START_TIMEOUT_S: u32 = 90;
/// Cota superior del apagado tras MASTER OFF: el ECB manda un shutdown ordenado
/// (cooldown + spool-down) antes de retirar AVAIL.
const APU_STOP_TIMEOUT_S: u32 = 180;

fn value(sim: &Sim, var: &str) -> f64 {
    sim.get(&[var]).unwrap()[var]
}

fn find<'a>(ecam: &'a [Warning], id: &str) -> Option<&'a Warning> {
    ecam.iter().find(|w| w.id == id)
}

/// Espera acotada: avanza en pasos de 1 s hasta que `pred` se cumpla.
fn run_until(sim: &mut Sim, timeout_s: u32, what: &str, pred: impl Fn(&Sim) -> bool) {
    let mut elapsed = 0;
    while !pred(sim) {
        sim.run(1.0, 5.0);
        elapsed += 1;
        assert!(elapsed <= timeout_s, "timeout ({timeout_s} s): {what}");
    }
}

/// Baterías dentro: lo mínimo para arrancar el APU (el combustible ya viene
/// del seed por defecto del runtime, slice 3 de Fase 4, #57).
///
/// La bomba hidráulica amarilla se aparca en AUTO (AUTO/ON invertido sin
/// seeding, D-007) para que no meta transitorios hidráulicos en un escenario
/// que no va de eso — mismo preámbulo que `generator_caution.rs`.
fn batteries_on() -> Sim {
    let mut sim = Sim::new();
    sim.set("hyd_epump_yellow", 1.0).unwrap();
    sim.set("bat_1", 1.0).unwrap();
    sim.set("bat_2", 1.0).unwrap();
    sim.run(3.0, 5.0);
    sim
}

/// Arranca el APU por los controles del catálogo y espera al AVAIL.
fn apu_available() -> Sim {
    let mut sim = batteries_on();
    sim.set(APU_MASTER, 1.0).unwrap();
    sim.run(1.0, 5.0);
    sim.set(APU_START, 1.0).unwrap();
    run_until(
        &mut sim,
        APU_START_TIMEOUT_S,
        "el APU no llegó a available",
        |s| value(s, APU_AVAILABLE) != 0.0,
    );
    sim
}

// --- (1) Arranque por controles del catálogo ---------------------------------

#[test]
fn apu_starts_via_catalog_controls_and_reaches_available() {
    let mut sim = batteries_on();

    // Antes de tocar nada, el APU está apagado y no disponible.
    assert_eq!(
        value(&sim, APU_AVAILABLE),
        0.0,
        "precondición: el APU no debería estar disponible en frío"
    );

    // MASTER y START por nombre amigable — ni un LVAR crudo en el camino.
    sim.set(APU_MASTER, 1.0).unwrap();
    sim.run(1.0, 5.0);
    sim.set(APU_START, 1.0).unwrap();
    run_until(
        &mut sim,
        APU_START_TIMEOUT_S,
        "el APU no llegó a available",
        |s| value(s, APU_AVAILABLE) != 0.0,
    );
}

// --- (2) AVAIL en la ECAM ----------------------------------------------------

#[test]
fn apu_avail_shows_on_ecam_while_available() {
    let sim = apu_available();

    // Con solo baterías el DC ESS está vivo: la ECAM tiene pantalla que mostrar
    // (ver doc del módulo) y el memo AVAIL del vendor aparece.
    assert_eq!(
        value(&sim, "ELEC_DC_ESS_BUS_IS_POWERED"),
        1.0,
        "precondición del gate ECAM: el DC ESS debe estar vivo con baterías"
    );
    let ecam = sim.read_ecam();
    let avail = find(&ecam, "apu.avail")
        .unwrap_or_else(|| panic!("se esperaba el memo APU AVAIL, ECAM: {ecam:?}"));
    assert_eq!(avail.message, "APU AVAIL");
    assert_eq!(avail.severity, Severity::Advisory);
    assert_eq!(avail.source, EcamSource::VendorFlag);
}

// --- (3) MASTER OFF apaga el APU ---------------------------------------------

#[test]
fn master_off_shuts_the_apu_down_and_retires_avail() {
    let mut sim = apu_available();

    sim.set(APU_MASTER, 0.0).unwrap();
    run_until(
        &mut sim,
        APU_STOP_TIMEOUT_S,
        "el APU no se apagó tras MASTER OFF",
        |s| value(s, APU_AVAILABLE) == 0.0,
    );

    // Y el memo AVAIL se retira de la ECAM con él.
    let ecam = sim.read_ecam();
    assert!(
        find(&ecam, "apu.avail").is_none(),
        "APU apagado: el memo AVAIL debería retirarse, ECAM: {ecam:?}"
    );
}

// --- (4) El bleed abre la válvula de verdad ----------------------------------

#[test]
fn apu_bleed_on_opens_the_bleed_air_valve_and_off_closes_it() {
    let mut sim = apu_available();

    // APU disponible pero bleed OFF (default del store, 0): válvula cerrada.
    sim.run(2.0, 5.0);
    assert_eq!(
        value(&sim, BLEED_VALVE_OPEN),
        0.0,
        "con el bleed OFF la válvula debería estar cerrada"
    );

    // Bleed ON por nombre amigable: el ECB abre la válvula (MASTER ON, N>95% y
    // bleed ON son sus tres condiciones — las tres se cumplen aquí).
    sim.set(APU_BLEED, 1.0).unwrap();
    run_until(
        &mut sim,
        10,
        "la válvula de bleed del APU no abrió",
        |s| value(s, BLEED_VALVE_OPEN) != 0.0,
    );

    // Y bleed OFF la cierra.
    sim.set(APU_BLEED, 0.0).unwrap();
    run_until(
        &mut sim,
        10,
        "la válvula de bleed del APU no cerró",
        |s| value(s, BLEED_VALVE_OPEN) == 0.0,
    );
}
