//! Test de integración del slice 5 de Fase 4 (issue #59): **lo que un motor en
//! marcha habilita** a través de los sistemas — generador de motor en el
//! eléctrico, EDPs en el hidráulico, neumático del arranque — y las cadenas
//! entre sistemas (un fallo eléctrico con consecuencia hidráulica).
//!
//! ## Semántica del vendor validada aquí (rutas en `core-rs/vendor/aircraft`)
//!
//! - **GEN 1 exige el pulsador GEN 1 LINE** del panel EMER ELEC además del GEN
//!   1: `gen_1_provides_power` (`a320_systems/src/electrical/
//!   alternating_current.rs:432-435`); el GEN 2 no tiene condición equivalente
//!   (`:436-438`). El vendor construye el pulsador en reposo ON
//!   (`electrical/mod.rs:391`, `new_on`) y el runtime lo siembra así (D-021).
//! - **EDPs**: `LeapEngine::hydraulic_pump_output_speed` deriva del N2 que
//!   generamos (`fbw-common/.../engine/leap_engine.rs:61-69`); con la bomba en
//!   AUTO el circuito llega a ~3000 psi sin bombas eléctricas ni PTU.
//! - **Overheat de la EDP** (`FailureType::EnginePumpOverheat`): calienta la
//!   bomba mientras gira a >200 RPM (`fbw-common/.../hydraulic/mod.rs:
//!   2808-2813`, tau aleatoria 30±5 s, `:2785-2796`); el calor pasa al fluido
//!   y de ahí al reservorio si hay flujo de retorno (>0.01 gal/s,
//!   `:2330-2334`), que escribe `HYD_GREEN_RESERVOIR_OVHT` (`:2273`). El fault
//!   del pulsador ENG 1 PUMP llega por esa vía indirecta:
//!   `has_overheat_fault = reservoir.is_overheating()`
//!   (`a320_systems/src/hydraulic/mod.rs:3052`, agregado al pulsador en
//!   `:1952`) — **no** directamente del failure.
//! - **PTU-inhibit con un solo master**: `should_enable_if_powered` exige PTU
//!   en AUTO y (ambos masters ON, o ambos OFF, o freno de parking suelto sin
//!   bypass pin) (`a320_systems/src/hydraulic/mod.rs:3491-3497`); sin
//!   alimentación el PTU va SIEMPRE a ON (`:3499-3500`, fail-safe del avión
//!   real). El pin de la dirección del morro lo gobierna `PUSHBACK STATE`
//!   (3 = sin pushback; el runtime lo siembra, D-021).
//! - **Neumático del arranque**: con la red alimentada y el selector X BLEED
//!   en AUTO (seed D-021), la válvula de crossbleed abre al abrir la de APU
//!   bleed (`a320_systems/src/pneumatic.rs:986-1008`) y el starter del motor 2
//!   recibe el aire del APU.

use a320_sim_core::api::Sim;
use a320_sim_core::ecam::{EcamSource, Severity, Warning};
use a320_sim_core::failures::FailureGroup;

// --- Outputs (escriben los sistemas) -----------------------------------------
const AC_1: &str = "ELEC_AC_1_BUS_IS_POWERED";
const AC_2: &str = "ELEC_AC_2_BUS_IS_POWERED";
const AC_ESS: &str = "ELEC_AC_ESS_BUS_IS_POWERED";
const DC_1: &str = "ELEC_DC_1_BUS_IS_POWERED";
const DC_2: &str = "ELEC_DC_2_BUS_IS_POWERED";
const DC_ESS: &str = "ELEC_DC_ESS_BUS_IS_POWERED";
const GEN_1_FAULT: &str = "OVHD_ELEC_ENG_GEN_1_PB_HAS_FAULT";
const GREEN_PRESSURE: &str = "HYD_GREEN_SYSTEM_1_SECTION_PRESSURE";
const BLUE_PRESSURE: &str = "HYD_BLUE_SYSTEM_1_SECTION_PRESSURE";
const YELLOW_PRESSURE: &str = "HYD_YELLOW_SYSTEM_1_SECTION_PRESSURE";
const PTU_VALVE: &str = "HYD_PTU_VALVE_OPENED";
const EDP_1_FAULT: &str = "OVHD_HYD_ENG_1_PUMP_PB_HAS_FAULT";
const GREEN_RESERVOIR_OVHT: &str = "HYD_GREEN_RESERVOIR_OVHT";
const ENG_1_STATE: &str = "ENGINE_STATE:1";
const ENG_2_STATE: &str = "ENGINE_STATE:2";
const ENG_2_STARTER_PRESSURIZED: &str = "PNEU_ENG_2_STARTER_PRESSURIZED";
const APU_AVAILABLE: &str = "OVHD_APU_START_PB_IS_AVAILABLE";

/// Presión de circuito presurizado nominal (~3000 psi).
const NOMINAL_PSI: std::ops::RangeInclusive<f64> = 2800.0..=3100.0;
/// Umbral LO PR del low press switch del vendor.
const LO_PR_PSI: f64 = 1450.0;
/// Presión de circuito muerto (ruido numérico aparte).
const DEPRESSURISED_PSI: f64 = 100.0;

fn value(sim: &Sim, var: &str) -> f64 {
    sim.get(&[var]).unwrap()[var]
}

fn find<'a>(ecam: &'a [Warning], id: &str) -> Option<&'a Warning> {
    ecam.iter().find(|w| w.id == id)
}

fn no_cautions(ecam: &[Warning]) -> bool {
    ecam.iter().all(|w| w.severity == Severity::Advisory)
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

/// Motor 1 al ralentí alimentando la red entera: baterías + bus tie, APU +
/// bleed (el gate del arranque exige aire real, #59), GEN 1 ON y EDP verde en
/// AUTO. Sin ext pwr y con el APU GEN en OFF (default D-007): la única fuente
/// que alimenta la red AC es el generador del motor 1.
fn engine_1_powering_the_network() -> Sim {
    let mut sim = Sim::new();
    sim.set("hyd_epump_yellow", 1.0).unwrap(); // aparcar en AUTO (D-007)
    sim.set("bat_1", 1.0).unwrap();
    sim.set("bat_2", 1.0).unwrap();
    sim.set("bus_tie", 1.0).unwrap();
    sim.run(3.0, 5.0);
    sim.set("apu_master", 1.0).unwrap();
    sim.run(1.0, 5.0);
    sim.set("apu_start", 1.0).unwrap();
    run_until(&mut sim, 120, "el APU no llegó a available", |s| {
        value(s, APU_AVAILABLE) != 0.0
    });
    sim.set("apu_bleed", 1.0).unwrap();
    sim.run(2.0, 5.0);
    sim.set("gen_1", 1.0).unwrap();
    sim.set("hyd_eng_1_pump", 1.0).unwrap();
    sim.set("eng_mode", 2.0).unwrap();
    sim.set("eng_master_1", 1.0).unwrap();
    run_until(&mut sim, 90, "el motor 1 no llegó a On", |s| {
        value(s, ENG_1_STATE) == 1.0
    });
    sim.run(5.0, 5.0); // asentar red y presiones
    sim
}

// --- (a) Motor 1 + GEN 1 ⇒ red AC entera, sin APU GEN ni ext pwr -------------

#[test]
fn engine_1_generator_powers_the_whole_network_without_apu_gen_or_ext_pwr() {
    let mut sim = engine_1_powering_the_network();

    // Precondición: ninguna otra fuente AC está comandada (el APU corre solo
    // para el bleed; su generador está en OFF, default D-007).
    assert_eq!(value(&sim, "OVHD_ELEC_APU_GEN_PB_IS_ON"), 0.0);
    assert_eq!(value(&sim, "OVHD_ELEC_EXT_PWR_PB_IS_ON"), 0.0);

    // El GEN 1 alimenta la red entera (AC 2 y DC 2 vía bus tie).
    for bus in [AC_1, AC_2, AC_ESS, DC_1, DC_2, DC_ESS] {
        assert!(
            value(&sim, bus) != 0.0,
            "{bus} debería estar alimentado por el GEN 1"
        );
    }
    assert_eq!(value(&sim, GEN_1_FAULT), 0.0, "sin fault en el GEN 1");
    // ECAM sin cautions (queda el advisory APU AVAIL: el APU sigue en marcha).
    let ecam = sim.read_ecam();
    assert!(
        no_cautions(&ecam),
        "red sana por el GEN 1: sin cautions, fue {ecam:?}"
    );

    // El gotcha del GEN 1 LINE, ahora como control: apagarlo aísla el GEN 1
    // (gen_1_provides_power exige la línea, alternating_current.rs:432-435) y
    // la red cae con su caution; encenderlo la recupera.
    sim.set("gen_1_line", 0.0).unwrap();
    sim.run(3.0, 5.0);
    assert_eq!(value(&sim, AC_1), 0.0, "GEN 1 LINE OFF aísla el GEN 1");
    let ecam = sim.read_ecam();
    let fault = find(&ecam, "elec.eng_gen.1.fault")
        .unwrap_or_else(|| panic!("se esperaba ENG 1 GEN FAULT, ECAM: {ecam:?}"));
    assert_eq!(fault.message, "ENG 1 GEN FAULT");
    assert_eq!(fault.severity, Severity::Caution);

    sim.set("gen_1_line", 1.0).unwrap();
    sim.run(3.0, 5.0);
    assert!(value(&sim, AC_1) != 0.0, "GEN 1 LINE ON recupera la red");
    assert!(no_cautions(&sim.read_ecam()));
}

// --- (b) Cada EDP presuriza su circuito, sin bombas eléctricas ni PTU --------

#[test]
fn each_engine_driven_pump_pressurises_its_own_circuit_alone() {
    let mut sim = engine_1_powering_the_network();

    // Verde a nominal por la EDP del motor 1: las bombas eléctricas están
    // paradas (amarilla en AUTO en tierra, azul en OFF default) y el PTU en
    // OFF (default D-007) con su controlador alimentado ⇒ válvula cerrada.
    let green = value(&sim, GREEN_PRESSURE);
    assert!(
        NOMINAL_PSI.contains(&green),
        "EDP verde: presión esperada en {NOMINAL_PSI:?} psi, fue {green}"
    );
    assert_eq!(value(&sim, PTU_VALVE), 0.0, "PTU OFF con controlador vivo");
    assert!(
        value(&sim, BLUE_PRESSURE) < DEPRESSURISED_PSI,
        "azul muerto: su bomba eléctrica está en OFF"
    );
    // El amarillo pudo recibir algo del PTU mientras el controlador estuvo sin
    // alimentar durante el arranque (fail-safe ON, hydraulic/mod.rs:3499-3500);
    // con la válvula ya cerrada queda muy por debajo del umbral LO PR.
    assert!(
        value(&sim, YELLOW_PRESSURE) < LO_PR_PSI,
        "amarillo sin fuente comandada tras cerrar el PTU"
    );

    // Motor 2 con aire del APU vía crossbleed (X BLEED en AUTO, seed D-021):
    // el starter 2 se presuriza y el motor 2 arranca.
    sim.set("hyd_eng_2_pump", 1.0).unwrap();
    sim.set("eng_master_2", 1.0).unwrap();
    run_until(
        &mut sim,
        20,
        "el starter del motor 2 no recibió aire",
        |s| value(s, ENG_2_STARTER_PRESSURIZED) != 0.0,
    );
    run_until(&mut sim, 90, "el motor 2 no llegó a On", |s| {
        value(s, ENG_2_STATE) == 1.0
    });
    run_until(
        &mut sim,
        30,
        "la EDP amarilla no presurizó su circuito",
        |s| value(s, YELLOW_PRESSURE) >= *NOMINAL_PSI.start(),
    );

    // Simétrico completo: cada circuito por su propia EDP, azul sigue muerto,
    // PTU cerrado (con ambos masters ON el PTU en OFF sigue mandando).
    let green = value(&sim, GREEN_PRESSURE);
    let yellow = value(&sim, YELLOW_PRESSURE);
    assert!(
        NOMINAL_PSI.contains(&green),
        "verde por su EDP: {green} psi"
    );
    assert!(
        NOMINAL_PSI.contains(&yellow),
        "amarillo por su EDP: {yellow} psi"
    );
    assert!(value(&sim, BLUE_PRESSURE) < DEPRESSURISED_PSI);
    assert_eq!(value(&sim, PTU_VALVE), 0.0);
    assert!(no_cautions(&sim.read_ecam()));
}

// --- (c) Overheat de la EDP verde ⇒ fault del pulsador y caution -------------

/// El fallo `hyd.eng_pump_overheat.green` no levanta el fault directamente: la
/// bomba se calienta girando (>200 RPM), el calor pasa al fluido y de ahí al
/// reservorio con flujo de retorno, y es el **reservorio recalentado** quien
/// enciende el fault del pulsador (ver doc del módulo). El PTU en AUTO
/// transfiere hacia el amarillo y garantiza ese flujo de retorno continuo.
#[test]
fn green_edp_overheat_raises_the_pump_fault_via_the_reservoir_and_its_caution() {
    let mut sim = engine_1_powering_the_network();
    sim.set("hyd_ptu", 1.0).unwrap(); // AUTO: transfiere y genera flujo
    sim.run(5.0, 5.0);

    sim.inject_failure("hyd.eng_pump_overheat.green").unwrap();
    // Tau de calentamiento aleatoria (30±5 s la bomba, luego el reservorio):
    // medido, el fault llega en ~60-90 s; 240 s de margen sin colgar el CI.
    run_until(
        &mut sim,
        240,
        "el overheat no levantó el fault del pulsador ENG 1 PUMP",
        |s| value(s, EDP_1_FAULT) != 0.0,
    );
    assert_eq!(
        value(&sim, GREEN_RESERVOIR_OVHT),
        1.0,
        "el fault llega vía el reservorio recalentado (hydraulic/mod.rs:3052)"
    );

    // La ECAM lo cuenta con el flag del vendor.
    let ecam = sim.read_ecam();
    let fault = find(&ecam, "hyd.eng_1_pump.fault")
        .unwrap_or_else(|| panic!("se esperaba HYD ENG 1 PUMP FAULT, ECAM: {ecam:?}"));
    assert_eq!(fault.message, "HYD ENG 1 PUMP FAULT");
    assert_eq!(fault.severity, Severity::Caution);
    assert_eq!(fault.system, FailureGroup::Hyd);
    assert_eq!(fault.source, EcamSource::VendorFlag);

    // La EDP sigue bombeando (el overheat solo declucha tras ~2 min de daño,
    // fbw-common/.../hydraulic/mod.rs:2768-2769): el verde sigue presurizado.
    assert!(
        value(&sim, GREEN_PRESSURE) > LO_PR_PSI,
        "overheat ≠ pérdida de presión inmediata"
    );
}

// --- (d) PTU inhibido durante el arranque de un solo motor -------------------

/// Las cuatro ramas exactas de `should_enable_if_powered`
/// (`hydraulic/mod.rs:3491-3497`) con el controlador alimentado: ambos masters
/// OFF ⇒ habilitado (freno irrelevante); un solo master ON ⇒ habilitado SOLO
/// con el freno de parking suelto (y sin bypass pin, que el seed de
/// `PUSHBACK STATE` mantiene fuera — D-021); ambos ON ⇒ habilitado.
#[test]
fn ptu_is_inhibited_during_a_single_engine_start_only_with_parking_brake_set() {
    // Red por ext pwr (rápida, sin motores) y amarillo por su bomba eléctrica.
    let mut sim = Sim::new();
    sim.set("hyd_epump_yellow", 1.0).unwrap();
    sim.set("bat_1", 1.0).unwrap();
    sim.set("bat_2", 1.0).unwrap();
    sim.set("bus_tie", 1.0).unwrap();
    sim.set("ext_pwr_avail", 1.0).unwrap();
    sim.set("ext_pwr", 1.0).unwrap();
    sim.run(3.0, 5.0);
    sim.set("hyd_epump_yellow", 0.0).unwrap(); // ON (invertido)
    run_until(&mut sim, 30, "el amarillo no llegó a nominal", |s| {
        value(s, YELLOW_PRESSURE) >= *NOMINAL_PSI.start()
    });

    // Ambos masters OFF: el PTU en AUTO transfiere (rama `!eng_1 && !eng_2`),
    // freno puesto o no.
    sim.set("hyd_ptu", 1.0).unwrap();
    run_until(&mut sim, 60, "el PTU no levantó el verde", |s| {
        value(s, GREEN_PRESSURE) > LO_PR_PSI
    });
    assert_eq!(value(&sim, PTU_VALVE), 1.0);
    sim.set("park_brake", 1.0).unwrap();
    sim.run(2.0, 5.0);
    assert_eq!(
        value(&sim, PTU_VALVE),
        1.0,
        "con ambos masters OFF el freno no inhibe"
    );

    // Master 1 ON (arranque en curso; sin APU el gate deja el motor en
    // Starting con N2=0, pero el PTU mira el master, no el N2): inhibido.
    sim.set("eng_mode", 2.0).unwrap();
    sim.set("eng_master_1", 1.0).unwrap();
    sim.run(2.0, 5.0);
    assert_eq!(value(&sim, ENG_1_STATE), 2.0, "arranque en curso");
    assert_eq!(
        value(&sim, PTU_VALVE),
        0.0,
        "un solo master ON + freno puesto ⇒ PTU inhibido (arranque de motor)"
    );
    assert_eq!(
        value(&sim, "HYD_PTU_ON_ECAM_MEMO"),
        0.0,
        "sin transferencia no hay memo"
    );

    // Freno suelto con un solo master: la rama `!parking_brake && !bypass_pin`
    // vuelve a habilitar.
    sim.set("park_brake", 0.0).unwrap();
    sim.run(2.0, 5.0);
    assert_eq!(
        value(&sim, PTU_VALVE),
        1.0,
        "freno suelto ⇒ habilitado aunque haya un master ON"
    );

    // Ambos masters ON (freno puesto de nuevo): habilitado por la rama both-on.
    sim.set("park_brake", 1.0).unwrap();
    sim.set("eng_master_2", 1.0).unwrap();
    sim.run(2.0, 5.0);
    assert_eq!(
        value(&sim, PTU_VALVE),
        1.0,
        "ambos masters ON ⇒ habilitado con freno puesto"
    );
}

// --- (e) Fallo eléctrico con consecuencia hidráulica -------------------------

/// La cadena eléctrico→hidráulico del épico: con el GEN 1 como única fuente
/// AC, la bomba eléctrica azul (en AUTO bombea con un motor en marcha,
/// `hydraulic/mod.rs:3134-3142`) mantiene el azul a nominal. Al caer el GEN 1,
/// la red AC muere ⇒ la bomba azul se para y el azul se despresuriza — pero la
/// EDP verde es **mecánica** y el verde sigue a nominal con el motor girando.
/// La ECAM (viva por las baterías) cuenta la cascada entera; el LO PR azul de
/// nuestro catálogo NO aparece, porque la bomba ya no está comandada con AC
/// vivo — sin alimentación no está "averiada", está apagada.
#[test]
fn losing_gen_1_stops_the_blue_epump_but_not_the_green_edp() {
    let mut sim = engine_1_powering_the_network();
    sim.set("hyd_epump_blue", 1.0).unwrap(); // AUTO: bombea con motor en marcha
    run_until(&mut sim, 30, "el azul no llegó a nominal", |s| {
        value(s, BLUE_PRESSURE) >= *NOMINAL_PSI.start()
    });
    assert!(
        no_cautions(&sim.read_ecam()),
        "precondición: ECAM sin cautions"
    );

    sim.inject_failure("elec.gen.1").unwrap();
    run_until(
        &mut sim,
        60,
        "el azul no se despresurizó al caer su bomba",
        |s| value(s, BLUE_PRESSURE) < LO_PR_PSI,
    );

    // La red AC entera cayó (el GEN 1 era la única fuente)...
    assert_eq!(value(&sim, AC_1), 0.0);
    assert_eq!(value(&sim, AC_2), 0.0);
    // ...pero el motor sigue girando y su EDP mecánica mantiene el verde.
    assert_eq!(
        value(&sim, ENG_1_STATE),
        1.0,
        "el motor no depende de la red"
    );
    let green = value(&sim, GREEN_PRESSURE);
    assert!(
        green > 2500.0,
        "la EDP es mecánica: el verde debería seguir presurizado, fue {green} psi"
    );
    // El PTU queda sin alimentación y va a ON (fail-safe, :3499-3500).
    assert_eq!(value(&sim, PTU_VALVE), 1.0, "PTU fail-safe ON sin DC");

    // La cascada en la ECAM (viva por el DC ESS de las baterías): el fault del
    // GEN 1, el del AC ESS aguas abajo y el de la bomba azul del vendor.
    let ecam = sim.read_ecam();
    for (id, message) in [
        ("elec.eng_gen.1.fault", "ENG 1 GEN FAULT"),
        ("elec.ac_ess_feed.fault", "AC ESS BUS FAULT"),
        ("hyd.epump_b.fault", "HYD B ELEC PUMP FAULT"),
    ] {
        let w = find(&ecam, id)
            .unwrap_or_else(|| panic!("se esperaba '{message}' en la cascada, ECAM: {ecam:?}"));
        assert_eq!(w.message, message);
        assert_eq!(w.severity, Severity::Caution);
        assert_eq!(w.source, EcamSource::VendorFlag);
    }
    // Nuestro LO PR azul exige la bomba comandada CON su AC vivo: sin red no
    // hay "avería de presión", hay bomba apagada.
    assert!(
        find(&ecam, "hyd.b.lo_pr").is_none(),
        "sin AC la bomba azul no está comandada: no debe haber LO PR, ECAM: {ecam:?}"
    );
}
