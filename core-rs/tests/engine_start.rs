//! Test de integración del slice de motores (issues #58/#59, Fase 4).
//!
//! El motor es **nuestro** (`src/engine.rs`, D-019/D-020): el Rust de FBW solo
//! *lee* los simvars de motor (N1/N2/estado/empuje/starter) como entrada pura,
//! y nadie los genera headless. Aquí se valida el contrato end-to-end a través
//! de la API: determinismo, timeline de arranque, gate de bleed, shutdown y
//! neutralidad (sin master o sin IGN/START nada se mueve).
//!
//! ## Semántica del vendor validada aquí (rutas en `core-rs/vendor/aircraft`)
//!
//! - `LeapEngine` lee `TURB ENG CORRECTED N1/N2:{n}`, `ENGINE_N2:{n}` y
//!   `TURB ENG JET THRUST:{n}` (`fbw-common/.../engine/leap_engine.rs:42-45,
//!   72-78`; el framework lee `Ratio` como percent y `Mass` como pound,
//!   `simulation/mod.rs:774,781`).
//! - El FADEC de pneumatic lee `ENGINE_STATE:{n}` (enum Off=0/On=1/Starting=2/
//!   Restarting=3/Shutting=4, `fbw-common/.../pneumatic/mod.rs:507-528`) y el
//!   selector `TURB ENG IGNITION SWITCH EX1:1` (CRANK=0/NORM=1/IGN-START=2,
//!   `:764-782`) — `a320_systems/src/pneumatic.rs:1587-1650`.
//! - **Gate de bleed (slice 5, #59)**: el motoring exige
//!   `PNEU_ENG_{n}_STARTER_PRESSURIZED` = 1, un output del vendor con
//!   histéresis de 10/5 psi sobre ambiente sobre la presión real del
//!   contenedor del starter (`a320_systems/src/pneumatic.rs:1278-1288`, write
//!   en `:1438-1441`). La válvula de arranque abre al leer nuestro
//!   `ENGINE_STATE = Starting` con N2 < 65 % (`:458-473`), y el ducto solo se
//!   presuriza con una fuente aguas arriba: el APU bleed a 50 psi con la
//!   turbina en marcha (`fbw-common/.../apu/aps3200.rs:422-425`).
//! - El PTU lee `GENERAL ENG STARTER ACTIVE:{n}` como eng master on/off
//!   (`a320_systems/src/hydraulic/mod.rs:3449-3452,3550-3554`).
//!
//! ## Nota de determinismo (cambia con el gate, #59)
//!
//! El instante en que llega el aire hereda el azar real del vendor: el tiempo
//! de apertura del flap de admisión del APU se sortea entre 6 y 12 s en
//! construcción (`fbw-common/.../apu/air_intake_flap.rs:21-31`,
//! `shared/random.rs`) y el EGT del APS3200 también
//! (`apu/aps3200.rs:248,322,353`). Por eso el test de determinismo ancla la
//! comparación en el primer tick de motoring de cada avión: desde ahí, la
//! trayectoria es una función pura del `dt` (D-019) y se exige igualdad f64
//! exacta.

use a320_sim_core::api::Sim;

const ENG_1_STATE: &str = "ENGINE_STATE:1";
const ENG_1_N2: &str = "ENGINE_N2:1";
const ENG_1_CORRECTED_N1: &str = "TURB ENG CORRECTED N1:1";
const ENG_1_CORRECTED_N2: &str = "TURB ENG CORRECTED N2:1";
const ENG_1_THRUST: &str = "TURB ENG JET THRUST:1";
const ENG_1_STARTER: &str = "GENERAL ENG STARTER ACTIVE:1";
const ENG_1_STARTER_VALVE: &str = "PNEU_ENG_1_STARTER_VALVE_OPEN";
const ENG_1_STARTER_PRESSURIZED: &str = "PNEU_ENG_1_STARTER_PRESSURIZED";
const ENG_2_STATE: &str = "ENGINE_STATE:2";
const ENG_2_N2: &str = "ENGINE_N2:2";
const MODE_SELECTOR: &str = "TURB ENG IGNITION SWITCH EX1:1";
const APU_AVAILABLE: &str = "OVHD_APU_START_PB_IS_AVAILABLE";

// Valores del enum `EngineState` del vendor.
const STATE_OFF: f64 = 0.0;
const STATE_ON: f64 = 1.0;
const STATE_STARTING: f64 = 2.0;
const STATE_SHUTTING: f64 = 4.0;

/// Cota superior del arranque del APU (el APS3200 llega a available a los
/// ~56-62 s medidos; el margen cubre el sorteo del flap).
const APU_START_TIMEOUT_S: u32 = 120;

fn value(sim: &Sim, var: &str) -> f64 {
    sim.get(&[var]).unwrap()[var]
}

/// Avanza en pasos de 1 s (5 Hz) hasta que `pred` se cumpla; devuelve los
/// segundos transcurridos. Panic si se supera `timeout_s`.
fn run_until(sim: &mut Sim, timeout_s: u32, what: &str, pred: impl Fn(&Sim) -> bool) -> u32 {
    let mut elapsed = 0;
    while !pred(sim) {
        sim.run(1.0, 5.0);
        elapsed += 1;
        assert!(elapsed <= timeout_s, "timeout ({timeout_s} s): {what}");
    }
    elapsed
}

/// Preámbulo del arranque con aire real (gate de bleed, #59): baterías + APU +
/// APU bleed. La bomba amarilla se aparca en AUTO (AUTO/ON invertido sin
/// seeding, D-007), como en el resto de slices.
fn bleed_established() -> Sim {
    let mut sim = Sim::new();
    sim.set("hyd_epump_yellow", 1.0).unwrap();
    sim.set("bat_1", 1.0).unwrap();
    sim.set("bat_2", 1.0).unwrap();
    sim.run(3.0, 5.0);
    sim.set("apu_master", 1.0).unwrap();
    sim.run(1.0, 5.0);
    sim.set("apu_start", 1.0).unwrap();
    run_until(
        &mut sim,
        APU_START_TIMEOUT_S,
        "el APU no llegó a available",
        |s| value(s, APU_AVAILABLE) != 0.0,
    );
    sim.set("apu_bleed", 1.0).unwrap();
    sim.run(2.0, 5.0);
    sim
}

// --- (a) Gate de bleed: sin aire el arranque se arma pero no progresa --------

/// El resultado central del slice 5: el motoring exige aire real. Sin APU
/// bleed, master + IGN/START dejan el FADEC en `Starting` con la válvula de
/// arranque del vendor abierta, pero el ducto a ambiente (14.7 psi, flag
/// `STARTER_PRESSURIZED` = 0) y el N2 clavado en 0. Cuando el APU bleed llega,
/// el mismo arranque completa sin tocar nada más.
#[test]
fn without_apu_bleed_the_start_arms_but_motoring_does_not_progress() {
    let mut sim = Sim::new();
    sim.set("hyd_epump_yellow", 1.0).unwrap();
    sim.set("bat_1", 1.0).unwrap();
    sim.set("bat_2", 1.0).unwrap();
    sim.run(3.0, 5.0);

    sim.set("eng_mode", 2.0).unwrap();
    sim.set("eng_master_1", 1.0).unwrap();
    sim.run(30.0, 5.0);

    // La secuencia está armada y la válvula del vendor abierta...
    assert_eq!(value(&sim, ENG_1_STATE), STATE_STARTING);
    assert_eq!(value(&sim, ENG_1_STARTER_VALVE), 1.0, "válvula abierta");
    // ...pero no hay fuente aguas arriba: ducto a ambiente y N2 clavado.
    assert_eq!(value(&sim, ENG_1_STARTER_PRESSURIZED), 0.0, "sin aire");
    assert_eq!(value(&sim, ENG_1_N2), 0.0, "el starter no gira sin aire");

    // Llega el aire (APU + bleed): el arranque completa sin tocar el panel ENG.
    sim.set("apu_master", 1.0).unwrap();
    sim.run(1.0, 5.0);
    sim.set("apu_start", 1.0).unwrap();
    run_until(
        &mut sim,
        APU_START_TIMEOUT_S,
        "el APU no llegó a available",
        |s| value(s, APU_AVAILABLE) != 0.0,
    );
    sim.set("apu_bleed", 1.0).unwrap();
    run_until(
        &mut sim,
        90,
        "el motor no arrancó al llegar el aire",
        |s| value(s, ENG_1_STATE) == STATE_ON,
    );
    // Con el arranque completado, el vendor corta la válvula y el flag cae.
    assert_eq!(value(&sim, ENG_1_STARTER_VALVE), 0.0);
}

// --- (b) Determinismo: misma secuencia ⇒ misma trayectoria anclada -----------

/// Requisito del benchmark, ajustado al gate (#59): el instante en que llega el
/// aire hereda el azar del APU del vendor (ver doc del módulo), así que la
/// comparación se ancla en el **primer tick con N2 > 0** de cada avión. Desde
/// ese anclaje, dos aviones conducidos igual producen exactamente los mismos
/// valores de motor, tick a tick (igualdad f64 exacta). Se comparan solo los
/// simvars generados por NUESTRO modelo: el vendor mete azar real en variables
/// suyas (consumo eléctrico aleatorizado, `fbw-common/.../electrical/
/// consumption.rs:86-89`; parámetros sorteados en construcción,
/// `physics/mod.rs:91-96`), así que el snapshot completo no es comparable.
#[test]
fn the_same_command_sequence_yields_identical_anchored_engine_trajectories() {
    let watched = [
        ENG_1_STATE,
        ENG_1_N2,
        ENG_1_CORRECTED_N1,
        ENG_1_CORRECTED_N2,
        ENG_1_THRUST,
        ENG_1_STARTER,
    ];

    // Trayectoria de motor anclada: ticks de 200 ms desde el primer tick con
    // N2 > 0 (el aire acaba de llegar), 350 ticks = 70 s (arranque completo).
    let anchored_trajectory = || -> Vec<[f64; 6]> {
        let mut sim = bleed_established();
        sim.set("eng_mode", 2.0).unwrap();
        sim.set("eng_master_1", 1.0).unwrap();

        let mut anchor_ticks = 0;
        while value(&sim, ENG_1_N2) == 0.0 {
            sim.step(200);
            anchor_ticks += 1;
            assert!(anchor_ticks < 100, "el motoring no arrancó en 20 s");
        }
        let mut series = Vec::with_capacity(350);
        for _ in 0..350 {
            series.push(watched.map(|var| value(&sim, var)));
            sim.step(200);
        }
        assert_eq!(
            value(&sim, ENG_1_STATE),
            STATE_ON,
            "la trayectoria debe terminar al ralentí, no comparando ceros"
        );
        series
    };

    let a = anchored_trajectory();
    let b = anchored_trajectory();
    for (tick, (ta, tb)) in a.iter().zip(&b).enumerate() {
        for (var, (va, vb)) in watched.iter().zip(ta.iter().zip(tb)) {
            assert!(
                va == vb,
                "no determinista en el tick {tick} tras el anclaje: {var} = {va} vs {vb}"
            );
        }
    }
}

// --- (c) Timeline del arranque -----------------------------------------------

#[test]
fn master_on_with_ignition_start_walks_the_start_timeline_to_idle() {
    let mut sim = bleed_established();
    sim.set("eng_mode", 2.0).unwrap();
    sim.set("eng_master_1", 1.0).unwrap();

    // Un tick: el FADEC pasa a Starting y el master espeja en el simvar que el
    // PTU lee como eng master on.
    sim.run(0.2, 5.0);
    assert_eq!(value(&sim, ENG_1_STATE), STATE_STARTING);
    assert_eq!(value(&sim, ENG_1_STARTER), 1.0);

    // Light-off: 25 % de N2 se cruza en ~14.3 s de motoring (8·ln 6) más el
    // ~medio segundo que tarda el ducto en presurizarse tras abrir la válvula.
    let t_light_off = run_until(&mut sim, 30, "N2 no cruzó el light-off (25 %)", |s| {
        value(s, ENG_1_N2) >= 25.0
    });
    assert!(
        (10..=20).contains(&t_light_off),
        "light-off a los {t_light_off} s, se esperaba ~15 s"
    );
    assert!(
        value(&sim, ENG_1_N2) < 50.0,
        "25 % debe cruzarse antes que 50 %"
    );
    assert_eq!(value(&sim, ENG_1_STATE), STATE_STARTING);

    // Arranque completado (Starting → On con N2 ≥ 58) en 40-70 s totales.
    let t_idle = t_light_off
        + run_until(&mut sim, 70, "el motor no llegó a On", |s| {
            value(s, ENG_1_STATE) == STATE_ON
        });
    assert!(
        (40..=70).contains(&t_idle),
        "idle a los {t_idle} s, se esperaba 40-70 s"
    );

    // Asentado al ralentí: N2 ~58.5, N1 ~18.5, empuje ~1000 lb, corrected =
    // uncorrected (tierra, ISA).
    sim.run(20.0, 5.0);
    let n2 = value(&sim, ENG_1_N2);
    assert!((n2 - 58.5).abs() < 0.3, "N2 al ralentí = {n2}");
    assert_eq!(value(&sim, ENG_1_CORRECTED_N2), n2);
    let n1 = value(&sim, ENG_1_CORRECTED_N1);
    assert!((n1 - 18.5).abs() < 0.3, "N1 al ralentí = {n1}");
    let thrust = value(&sim, ENG_1_THRUST);
    assert!(
        (thrust - 1000.0).abs() < 10.0,
        "empuje al ralentí = {thrust} lb"
    );
    // El motor 2 no se ha movido: masters independientes.
    assert_eq!(value(&sim, ENG_2_STATE), STATE_OFF);
    assert_eq!(value(&sim, ENG_2_N2), 0.0);
}

// --- (d) Shutdown: master OFF ⇒ Shutting y decaimiento monótono --------------

#[test]
fn master_off_spools_the_engine_down_to_off() {
    let mut sim = bleed_established();
    sim.set("eng_mode", 2.0).unwrap();
    sim.set("eng_master_1", 1.0).unwrap();
    run_until(&mut sim, 90, "el motor no arrancó", |s| {
        value(s, ENG_1_STATE) == STATE_ON
    });

    sim.set("eng_master_1", 0.0).unwrap();
    sim.run(0.2, 5.0);
    assert_eq!(value(&sim, ENG_1_STATE), STATE_SHUTTING);
    assert_eq!(value(&sim, ENG_1_STARTER), 0.0, "master OFF espejado");

    // N2 decae monótonamente hasta Off (<1 % en ~49 s, 12·ln 58.5).
    let mut previous = value(&sim, ENG_1_N2);
    let mut elapsed = 0;
    while value(&sim, ENG_1_STATE) != STATE_OFF {
        sim.run(1.0, 5.0);
        let n2 = value(&sim, ENG_1_N2);
        assert!(
            n2 < previous,
            "el spool-down debe ser monótono ({n2} ≥ {previous})"
        );
        previous = n2;
        elapsed += 1;
        assert!(elapsed <= 90, "timeout: el motor no llegó a Off");
    }
    assert!(value(&sim, ENG_1_N2) < 1.0);
}

// --- (e) Neutralidad: sin master o sin IGN/START nada se mueve ---------------

#[test]
fn without_master_or_in_norm_mode_nothing_moves() {
    let mut sim = Sim::new();

    // El seed del runtime deja el selector descansando en NORM (=1), porque el
    // default de una var no escrita sería 0 = CRANK.
    assert_eq!(
        value(&sim, MODE_SELECTOR),
        1.0,
        "el selector descansa en NORM"
    );

    // Selector en IGN/START pero sin master: nada.
    sim.set("eng_mode", 2.0).unwrap();
    sim.run(10.0, 5.0);
    assert_eq!(value(&sim, ENG_1_STATE), STATE_OFF);
    assert_eq!(value(&sim, ENG_1_N2), 0.0);

    // Master ON pero selector devuelto a NORM: tampoco.
    sim.set("eng_mode", 1.0).unwrap();
    sim.set("eng_master_1", 1.0).unwrap();
    sim.run(10.0, 5.0);
    assert_eq!(value(&sim, ENG_1_STATE), STATE_OFF);
    assert_eq!(value(&sim, ENG_1_N2), 0.0);
    assert_eq!(value(&sim, ENG_2_STATE), STATE_OFF);
}

// --- (f) Registro: los LVARs del panel ENG existen tras un tick --------------

/// Hermano local de `every_catalog_lvar_is_registered_after_a_tick` (api.rs):
/// los `ENG_MASTER_{1,2}` son nuestros y solo existen porque el runtime los
/// siembra — si el seed se cayera, este test (y el del catálogo) lo delatan.
#[test]
fn engine_control_lvars_are_registered_from_the_start() {
    let mut sim = Sim::new();
    sim.step(1000);
    let vars = sim.list_variables();
    for lvar in ["ENG_MASTER_1", "ENG_MASTER_2", MODE_SELECTOR] {
        assert!(
            vars.iter().any(|n| n == lvar),
            "'{lvar}' no está en el registro tras un tick"
        );
    }
}
