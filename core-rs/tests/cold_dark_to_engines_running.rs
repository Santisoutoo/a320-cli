//! Test de cierre de la Fase 4 (issue #60, épico #18): la secuencia completa
//! **cold & dark → engines running**, solo con nombres amigables del catálogo
//! y con un checkpoint verificado tras cada paso.
//!
//! Este test es la Definition of Done del épico y a la vez la documentación
//! ejecutable de la secuencia que reproducen el humano (CLI) y el LLM (MCP,
//! `--start engines-running`). Cada paso está validado por su slice previo:
//!
//! 1. Baterías (slice eléctrico, Fase 1) → DC vivo.
//! 2. APU master + start (slice 2, #56) → AVAIL en ~62 s.
//! 3. APU GEN + **bus tie** (Fase 2/#16): sin el bus tie el APU GEN no
//!    alimenta nada — y el BMC que abre el crossbleed va colgado de esa red.
//! 4. APU bleed (slice 2): aire para el arranque (gate de bleed, #59).
//! 5. Motor 1: IGN/START + master (slices 4/5, #58/#59) → idle en ~55 s;
//!    luego GEN 1.
//! 6. Motor 2: bebe del crossbleed (X BLEED en AUTO, seed D-021) → GEN 2.
//! 7. Recogida: NORM, APU bleed/gen/master OFF → red entera por los ENG GEN,
//!    EDPs presurizando verde y amarillo, azul por su bomba eléctrica en AUTO.
//!
//! La semántica del vendor que cada paso ejercita está citada en el test de su
//! slice (`electrical_slice.rs`, `apu_slice.rs`, `engine_start.rs`,
//! `engine_cross_system.rs`); aquí no se afirma nada nuevo del vendor, solo se
//! encadena lo ya verificado.

use a320_sim_core::api::Sim;
use a320_sim_core::ecam::{Severity, Warning};

// --- Outputs (escriben los sistemas) -----------------------------------------
const AC_1: &str = "ELEC_AC_1_BUS_IS_POWERED";
const AC_2: &str = "ELEC_AC_2_BUS_IS_POWERED";
const AC_ESS: &str = "ELEC_AC_ESS_BUS_IS_POWERED";
const DC_1: &str = "ELEC_DC_1_BUS_IS_POWERED";
const DC_2: &str = "ELEC_DC_2_BUS_IS_POWERED";
const DC_ESS: &str = "ELEC_DC_ESS_BUS_IS_POWERED";
const DC_BAT: &str = "ELEC_DC_BAT_BUS_IS_POWERED";
const GEN_1_FAULT: &str = "OVHD_ELEC_ENG_GEN_1_PB_HAS_FAULT";
const GEN_2_FAULT: &str = "OVHD_ELEC_ENG_GEN_2_PB_HAS_FAULT";
const APU_AVAILABLE: &str = "OVHD_APU_START_PB_IS_AVAILABLE";
const APU_BLEED_VALVE: &str = "APU_BLEED_AIR_VALVE_OPEN";
const ENG_1_STATE: &str = "ENGINE_STATE:1";
const ENG_2_STATE: &str = "ENGINE_STATE:2";
const ENG_1_N2: &str = "ENGINE_N2:1";
const ENG_2_N2: &str = "ENGINE_N2:2";
const ENG_1_STARTER_PRESSURIZED: &str = "PNEU_ENG_1_STARTER_PRESSURIZED";
const ENG_2_STARTER_PRESSURIZED: &str = "PNEU_ENG_2_STARTER_PRESSURIZED";
const GREEN_PRESSURE: &str = "HYD_GREEN_SYSTEM_1_SECTION_PRESSURE";
const BLUE_PRESSURE: &str = "HYD_BLUE_SYSTEM_1_SECTION_PRESSURE";
const YELLOW_PRESSURE: &str = "HYD_YELLOW_SYSTEM_1_SECTION_PRESSURE";
const PTU_VALVE: &str = "HYD_PTU_VALVE_OPENED";
const PTU_MEMO: &str = "HYD_PTU_ON_ECAM_MEMO";

// Valores del enum `EngineState` del vendor (ver `engine_start.rs`).
const STATE_OFF: f64 = 0.0;
const STATE_ON: f64 = 1.0;

/// Presión de circuito presurizado nominal (~3000 psi).
const NOMINAL_PSI: std::ops::RangeInclusive<f64> = 2800.0..=3100.0;

/// Cotas de las esperas, en segundos de simulación. Medido: AVAIL a ~62 s,
/// cada motor a idle en ~55 s, y el apagado ordenado del APU (cooldown de
/// bleed + spool-down) en ~130 s tras MASTER OFF.
const APU_START_TIMEOUT_S: u32 = 120;
const ENGINE_START_TIMEOUT_S: u32 = 90;
const APU_STOP_TIMEOUT_S: u32 = 300;

fn value(sim: &Sim, var: &str) -> f64 {
    sim.get(&[var]).unwrap()[var]
}

fn powered(sim: &Sim, bus: &str) -> bool {
    value(sim, bus) != 0.0
}

fn no_cautions(ecam: &[Warning]) -> bool {
    ecam.iter().all(|w| w.severity == Severity::Advisory)
}

/// Espera acotada: avanza en pasos de 1 s (5 Hz) hasta que `pred` se cumpla;
/// devuelve los segundos transcurridos. Panic si supera `timeout_s`.
fn run_until(sim: &mut Sim, timeout_s: u32, what: &str, pred: impl Fn(&Sim) -> bool) -> u32 {
    let mut elapsed = 0;
    while !pred(sim) {
        sim.run(1.0, 5.0);
        elapsed += 1;
        assert!(elapsed <= timeout_s, "timeout ({timeout_s} s): {what}");
    }
    elapsed
}

#[test]
fn cold_and_dark_to_engines_running_with_catalog_names_only() {
    let mut sim = Sim::new();

    // Preparación de panel: los reposos que el runtime aún no siembra (deuda
    // anotada en D-021). La bomba amarilla eléctrica se aparca en AUTO
    // (AUTO/ON invertido sin seed, D-007); la azul en AUTO (bombea sola con un
    // motor en marcha) y el PTU en AUTO, como descansan en el avión real.
    sim.set("hyd_epump_yellow", 1.0).unwrap();
    sim.set("hyd_epump_blue", 1.0).unwrap();
    sim.set("hyd_ptu", 1.0).unwrap();

    // Cold & dark de verdad: nada alimentado antes de tocar el panel ELEC.
    sim.run(1.0, 5.0);
    assert!(!powered(&sim, DC_BAT), "cold & dark: DC BAT muerto");
    assert!(!powered(&sim, AC_1), "cold & dark: AC 1 muerto");

    // --- (1) Baterías: el DC cobra vida ------------------------------------
    sim.set("bat_1", 1.0).unwrap();
    sim.set("bat_2", 1.0).unwrap();
    sim.run(2.0, 5.0);
    assert!(powered(&sim, DC_BAT), "baterías dentro: DC BAT vivo");
    assert!(powered(&sim, DC_ESS), "baterías dentro: DC ESS vivo");
    assert!(!powered(&sim, AC_1), "sin fuente AC todavía");
    assert!(!powered(&sim, AC_2), "sin fuente AC todavía");

    // --- (2) APU: master, start y espera acotada al AVAIL -------------------
    sim.set("apu_master", 1.0).unwrap();
    sim.run(2.0, 5.0);
    sim.set("apu_start", 1.0).unwrap();
    let t_apu = run_until(
        &mut sim,
        APU_START_TIMEOUT_S,
        "el APU no llegó a available",
        |s| value(s, APU_AVAILABLE) != 0.0,
    );
    assert!(
        (40..=110).contains(&t_apu),
        "AVAIL a los {t_apu} s, se esperaba ~62 s"
    );
    // La ECAM (viva por el DC ESS de las baterías) muestra el memo AVAIL.
    let ecam = sim.read_ecam();
    assert!(
        ecam.iter().any(|w| w.id == "apu.avail"),
        "se esperaba APU AVAIL en la ECAM: {ecam:?}"
    );
    assert!(!powered(&sim, AC_1), "APU disponible pero su GEN aún OFF");

    // --- (3) APU GEN + bus tie: la red AC entera cobra vida -----------------
    // El bus tie es imprescindible: sin él el APU GEN no alimenta AC 1/2 y el
    // BMC del crossbleed (paso 6) se queda sin alimentación.
    sim.set("apu_gen", 1.0).unwrap();
    sim.set("bus_tie", 1.0).unwrap();
    sim.run(2.0, 5.0);
    for bus in [AC_1, AC_2, AC_ESS, DC_1, DC_2, DC_ESS, DC_BAT] {
        assert!(powered(&sim, bus), "{bus} alimentado por el APU GEN");
    }
    assert!(
        no_cautions(&sim.read_ecam()),
        "red sana por el APU GEN: sin cautions"
    );

    // --- (4) APU bleed: aire para el arranque -------------------------------
    sim.set("apu_bleed", 1.0).unwrap();
    sim.run(2.0, 5.0);
    assert_eq!(
        value(&sim, APU_BLEED_VALVE),
        1.0,
        "la válvula de APU bleed debería estar abierta"
    );

    // --- (5) Motor 1: IGN/START + master, luego su generador ----------------
    sim.set("eng_mode", 2.0).unwrap();
    sim.set("eng_master_1", 1.0).unwrap();
    run_until(
        &mut sim,
        20,
        "el starter del motor 1 no recibió aire del APU",
        |s| value(s, ENG_1_STARTER_PRESSURIZED) != 0.0,
    );
    let t_eng_1 = run_until(
        &mut sim,
        ENGINE_START_TIMEOUT_S,
        "el motor 1 no llegó a On",
        |s| value(s, ENG_1_STATE) == STATE_ON,
    );
    assert!(
        (40..=80).contains(&t_eng_1),
        "motor 1 a idle en {t_eng_1} s, se esperaba ~55 s"
    );
    assert_eq!(
        value(&sim, ENG_2_STATE),
        STATE_OFF,
        "masters independientes"
    );
    sim.set("gen_1", 1.0).unwrap();
    sim.run(2.0, 5.0);
    assert_eq!(value(&sim, GEN_1_FAULT), 0.0, "GEN 1 en línea sin fault");
    assert!(powered(&sim, AC_1), "AC 1 sigue vivo con el GEN 1 en línea");

    // --- (6) Motor 2: arranca con aire del crossbleed -----------------------
    // El X BLEED descansa en AUTO (seed D-021): la válvula de crossbleed abre
    // con la de APU bleed y el starter del motor 2 recibe el mismo aire.
    sim.set("eng_master_2", 1.0).unwrap();
    run_until(
        &mut sim,
        20,
        "el starter del motor 2 no recibió aire del crossbleed",
        |s| value(s, ENG_2_STARTER_PRESSURIZED) != 0.0,
    );
    let t_eng_2 = run_until(
        &mut sim,
        ENGINE_START_TIMEOUT_S,
        "el motor 2 no llegó a On",
        |s| value(s, ENG_2_STATE) == STATE_ON,
    );
    assert!(
        (40..=80).contains(&t_eng_2),
        "motor 2 a idle en {t_eng_2} s, se esperaba ~55 s"
    );
    sim.set("gen_2", 1.0).unwrap();
    sim.run(2.0, 5.0);
    assert_eq!(value(&sim, GEN_2_FAULT), 0.0, "GEN 2 en línea sin fault");

    // --- (7) Recogida: NORM y APU fuera -------------------------------------
    sim.set("eng_mode", 1.0).unwrap();
    sim.set("apu_bleed", 0.0).unwrap();
    sim.set("apu_gen", 0.0).unwrap();
    sim.set("apu_master", 0.0).unwrap();
    sim.run(5.0, 5.0);

    // La red no parpadea: los ENG GEN ya la llevaban (prioridad sobre el APU
    // GEN) y son ahora la única fuente comandada — ni APU GEN ni ext pwr.
    assert_eq!(value(&sim, "OVHD_ELEC_APU_GEN_PB_IS_ON"), 0.0);
    assert_eq!(value(&sim, "OVHD_ELEC_EXT_PWR_PB_IS_ON"), 0.0);
    for bus in [AC_1, AC_2, AC_ESS, DC_1, DC_2, DC_ESS, DC_BAT] {
        assert!(powered(&sim, bus), "{bus} alimentado por los ENG GEN");
    }
    assert_eq!(value(&sim, APU_BLEED_VALVE), 0.0, "APU bleed cerrado");

    // El apagado del APU es ordenado (cooldown + spool-down): AVAIL tarda
    // ~2 min en retirarse tras MASTER OFF. Se espera acotado para dejar la
    // ECAM final sin el memo.
    let t_apu_stop = run_until(
        &mut sim,
        APU_STOP_TIMEOUT_S,
        "el APU no se apagó tras MASTER OFF",
        |s| value(s, APU_AVAILABLE) == 0.0,
    );
    sim.run(5.0, 5.0);

    // --- (8) Estado final: engines running ----------------------------------
    // Motores al ralentí, cada uno On con N2 ~58.5 %.
    for (state, n2) in [(ENG_1_STATE, ENG_1_N2), (ENG_2_STATE, ENG_2_N2)] {
        assert_eq!(value(&sim, state), STATE_ON);
        let n2 = value(&sim, n2);
        assert!((n2 - 58.5).abs() < 0.5, "N2 al ralentí = {n2}");
    }
    assert_eq!(value(&sim, GEN_1_FAULT), 0.0);
    assert_eq!(value(&sim, GEN_2_FAULT), 0.0);

    // Hidráulica: verde y amarillo a nominal por sus EDPs (sembradas AUTO,
    // D-021), azul por su bomba eléctrica en AUTO con motores en marcha.
    let green = value(&sim, GREEN_PRESSURE);
    let yellow = value(&sim, YELLOW_PRESSURE);
    let blue = value(&sim, BLUE_PRESSURE);
    assert!(
        NOMINAL_PSI.contains(&green),
        "verde por su EDP: {green} psi"
    );
    assert!(
        NOMINAL_PSI.contains(&yellow),
        "amarillo por su EDP: {yellow} psi"
    );
    assert!(
        NOMINAL_PSI.contains(&blue),
        "azul por su bomba eléctrica en AUTO: {blue} psi"
    );

    // PTU en AUTO con ambos masters ON: habilitado (válvula abierta), pero con
    // verde y amarillo a presión pareja no transfiere — sin memo en la ECAM.
    assert_eq!(
        value(&sim, PTU_VALVE),
        1.0,
        "PTU habilitado con ambos motores"
    );
    assert_eq!(
        value(&sim, PTU_MEMO),
        0.0,
        "presiones parejas: sin transferencia"
    );

    // ECAM final: completamente limpia. El memo AVAIL se retiró con el APU,
    // el PTU no transfiere, y no queda ni una caution: el avión está sano.
    let ecam = sim.read_ecam();
    assert!(
        ecam.is_empty(),
        "engines running con el avión sano: ECAM limpia, fue {ecam:?}"
    );

    // Timings reales de la corrida (visibles con `--nocapture`), para calibrar
    // las cotas si el vendor o el modelo de motor cambian.
    eprintln!(
        "secuencia completa: APU available en {t_apu} s, motor 1 en {t_eng_1} s, \
         motor 2 en {t_eng_2} s, apagado del APU en {t_apu_stop} s"
    );
}
