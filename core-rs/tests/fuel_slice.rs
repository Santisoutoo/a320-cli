//! Test de integración del slice de combustible (issue #57, Fase 4 slice 3).
//!
//! El combustible es **estado de mundo sembrado una vez**: el Rust de FBW no
//! modela consumo ni crossfeed — los simvars `FUEL TANK * QUANTITY` son
//! entradas en galones US que `FuelTank::read` convierte a kg con
//! `FUEL_GALLONS_TO_KG` (`fbw-common/.../systems/src/fuel/mod.rs:12,97-100`) —
//! y el runtime los siembra en `Runtime::new` con una carga de corto radio
//! (~6 400 kg; ver `runtime::FUEL_SEED_GALLONS`). Este archivo cierra el bucle
//! que las muletas de `UNLIMITED FUEL` dejaban abierto: el APU ahora arranca
//! con combustible *real* y se cae si su tanque se vacía.
//!
//! ## Semántica del vendor validada aquí (rutas en `core-rs/vendor/aircraft`)
//!
//! - **Total**: `FuelSystem::write` publica `TOTAL_FUEL_QUANTITY` (kg) y
//!   `TOTAL_FUEL_VOLUME` (gal) cada tick
//!   (`fbw-common/.../systems/src/fuel/mod.rs:120-121,188-195`).
//! - **De qué tanque bebe el APU**: `left_inner_tank_has_fuel_remaining()`
//!   consulta `A320FuelTankType::LeftInner` = índice 1 = simvar
//!   `FUEL TANK LEFT MAIN QUANTITY` (`a320_systems/src/fuel/mod.rs:23-47,
//!   53-79,134-137`; el caller es `a320_systems/src/lib.rs:158-172`).
//! - **Auto-shutdown por fuel**: con N > 0 y el `FuelPressureSwitch` sin
//!   presión, el ECB levanta `ApuFault::FuelLowPressure`
//!   (`electronic_control_box.rs:224-230`); cualquier fault salvo `ApuFire` es
//!   `is_auto_shutdown()` (`:300-302`) y el ECB propaga el fault al MASTER SW
//!   (`apu/mod.rs:371`, `self.master.set_fault(apu.has_fault())`), que es lo
//!   que nuestra regla ECAM `apu.master.fault` ("APU FAULT") lee.

use a320_sim_core::api::Sim;
use a320_sim_core::ecam::{EcamSource, Severity, Warning};
use a320_sim_core::failures::FailureGroup;

// --- Simvars de tanque (entradas de mundo, en galones US) --------------------
const TANK_CENTER: &str = "FUEL TANK CENTER QUANTITY";
const TANK_LEFT_MAIN: &str = "FUEL TANK LEFT MAIN QUANTITY";
const TANK_LEFT_AUX: &str = "FUEL TANK LEFT AUX QUANTITY";
const TANK_RIGHT_MAIN: &str = "FUEL TANK RIGHT MAIN QUANTITY";
const TANK_RIGHT_AUX: &str = "FUEL TANK RIGHT AUX QUANTITY";

// --- Outputs (escriben los sistemas) -----------------------------------------
const TOTAL_FUEL_KG: &str = "TOTAL_FUEL_QUANTITY";
const APU_AVAILABLE: &str = "OVHD_APU_START_PB_IS_AVAILABLE";

/// Densidad del vendor (`fbw-common/.../systems/src/fuel/mod.rs:12`).
const FUEL_GALLONS_TO_KG: f64 = 3.039075693483925;

/// El seed por defecto en galones (espejo de `runtime::FUEL_SEED_GALLONS`):
/// aux llenos (228), mains a 825, center vacío = 2 106 gal ≈ 6 400 kg.
const SEED_TOTAL_GALLONS: f64 = 228.0 * 2.0 + 825.0 * 2.0;

/// Cota superior del arranque del APU (~62 s medidos; margen sin colgar el CI).
const APU_START_TIMEOUT_S: u32 = 90;
/// Cota del auto-shutdown tras vaciar el tanque: el fault es inmediato pero el
/// AVAIL solo cae cuando N baja del umbral durante el spool-down.
const APU_DIE_TIMEOUT_S: u32 = 120;

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

// --- (a) El seed por defecto es observable desde el primer instante ----------

#[test]
fn default_seed_shows_in_tank_reads_and_total_fuel_quantity() {
    let sim = Sim::new();

    // Sin ningún tick del caller: el seed se escribe antes del tick de
    // inicialización de `Runtime::new`, así que el primer estado ya es el de
    // un avión cargado.
    assert_eq!(value(&sim, TANK_CENTER), 0.0, "center vacío en corto radio");
    assert_eq!(value(&sim, TANK_LEFT_MAIN), 825.0);
    assert_eq!(value(&sim, TANK_LEFT_AUX), 228.0);
    assert_eq!(value(&sim, TANK_RIGHT_MAIN), 825.0);
    assert_eq!(value(&sim, TANK_RIGHT_AUX), 228.0);

    // Y el sistema de fuel del vendor ya publicó el total en kg.
    let expected_kg = SEED_TOTAL_GALLONS * FUEL_GALLONS_TO_KG;
    let total = value(&sim, TOTAL_FUEL_KG);
    assert!(
        (total - expected_kg).abs() < 0.5,
        "TOTAL_FUEL_QUANTITY = {total} kg, se esperaba ~{expected_kg:.1} kg (~6 400 kg)"
    );

    // La muleta está retirada de verdad: nadie escribe UNLIMITED FUEL.
    assert_eq!(value(&sim, "UNLIMITED FUEL"), 0.0);
}

// --- (b) Una escritura de tanque persiste: es seed, no entorno ---------------

#[test]
fn a_tank_write_persists_across_ticks_and_updates_the_total() {
    let mut sim = Sim::new();

    // Repostar el center por nombre amigable del catálogo (dominio World).
    sim.set("fuel_tank_center", 500.0).unwrap();
    sim.run(2.0, 5.0);

    // Nadie la machaca (el entorno NO reescribe fuel cada tick)...
    assert_eq!(
        value(&sim, TANK_CENTER),
        500.0,
        "la escritura de tanque debe sobrevivir a los ticks"
    );
    // ...y el total del vendor la refleja.
    let expected_kg = (SEED_TOTAL_GALLONS + 500.0) * FUEL_GALLONS_TO_KG;
    let total = value(&sim, TOTAL_FUEL_KG);
    assert!(
        (total - expected_kg).abs() < 0.5,
        "TOTAL_FUEL_QUANTITY = {total} kg, se esperaba ~{expected_kg:.1} kg"
    );

    // Y un valor por encima de la capacidad del tanque se rechaza.
    assert!(sim.set("fuel_tank_center", 5000.0).is_err());
}

// --- (c)+(d) El APU vive del fuel sembrado y muere sin él --------------------

#[test]
fn apu_runs_on_the_seeded_fuel_and_dies_when_its_tank_drains() {
    let mut sim = Sim::new();

    // Mismo preámbulo que apu_slice.rs, pero SIN `UNLIMITED FUEL`: el fuel es
    // el del seed. La bomba amarilla se aparca en AUTO (AUTO/ON invertido sin
    // seeding, D-007) para no meter transitorios hidráulicos.
    sim.set("hyd_epump_yellow", 1.0).unwrap();
    sim.set("bat_1", 1.0).unwrap();
    sim.set("bat_2", 1.0).unwrap();
    sim.run(3.0, 5.0);
    assert_eq!(
        value(&sim, "UNLIMITED FUEL"),
        0.0,
        "precondición: el arranque debe apoyarse en el seed, no en la muleta"
    );

    // (c) El APU arranca con el fuel sembrado (bebe del left main: 825 gal).
    sim.set("apu_master", 1.0).unwrap();
    sim.run(1.0, 5.0);
    sim.set("apu_start", 1.0).unwrap();
    run_until(
        &mut sim,
        APU_START_TIMEOUT_S,
        "el APU no llegó a available con el fuel sembrado",
        |s| value(s, APU_AVAILABLE) != 0.0,
    );

    // (d) Vaciar el tanque del que bebe (left main) con el APU corriendo: el
    // ECB pierde la presión de combustible y auto-apaga el APU.
    sim.set("fuel_tank_left_main", 0.0).unwrap();
    run_until(
        &mut sim,
        APU_DIE_TIMEOUT_S,
        "el APU no se cayó tras vaciar el left main",
        |s| value(s, APU_AVAILABLE) == 0.0,
    );

    // El fault del MASTER SW (FuelLowPressure) llega a la ECAM como la caution
    // "APU FAULT" — calculada por FBW, no por una regla nuestra — y el memo
    // AVAIL se retira con el APU.
    let ecam = sim.read_ecam();
    let fault = find(&ecam, "apu.master.fault")
        .unwrap_or_else(|| panic!("se esperaba la caution APU FAULT, ECAM: {ecam:?}"));
    assert_eq!(fault.message, "APU FAULT");
    assert_eq!(fault.severity, Severity::Caution);
    assert_eq!(fault.system, FailureGroup::Apu);
    assert_eq!(fault.source, EcamSource::VendorFlag);
    assert!(
        find(&ecam, "apu.avail").is_none(),
        "APU caído: el memo AVAIL debería retirarse, ECAM: {ecam:?}"
    );
}
