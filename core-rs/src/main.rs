//! Demo fina del vertical slice eléctrico sobre la API pública (`api::Sim`).
//!
//! El spike de Fase 0 vivía aquí y hablaba con `SimulationTestBed` directamente;
//! esa función la hereda ahora el test de integración (`tests/electrical_slice.rs`,
//! issue #13), que asertar el comportamiento en vez de imprimirlo. Este `main`
//! queda como demo legible para un humano: recorre cold & dark -> baterías ON ->
//! ext pwr y vuelca el estado de los buses, todo por la misma API que usan la
//! CLI y el MCP. No usa nada interno del avión ni el harness de tests.

use std::collections::BTreeMap;

use a320_sim_core::api::Sim;

const OBSERVED: &[&str] = &[
    "ELEC_AC_1_BUS_IS_POWERED",
    "ELEC_DC_1_BUS_IS_POWERED",
    "ELEC_DC_BAT_BUS_IS_POWERED",
    "ELEC_DC_HOT_1_BUS_IS_POWERED",
    "ELEC_TR_1_POTENTIAL_NORMAL",
];

fn report(label: &str, state: &BTreeMap<String, f64>) {
    let on = |name: &str| if state[name] != 0.0 { "ON " } else { "off" };
    println!(
        "{label:<16} AC_1={} DC_1={} DC_BAT={} DC_HOT_1={} TR_1_OK={}",
        on("ELEC_AC_1_BUS_IS_POWERED"),
        on("ELEC_DC_1_BUS_IS_POWERED"),
        on("ELEC_DC_BAT_BUS_IS_POWERED"),
        on("ELEC_DC_HOT_1_BUS_IS_POWERED"),
        on("ELEC_TR_1_POTENTIAL_NORMAL"),
    );
}

/// Vuelca la ECAM tal como la vería el agente (Fase 2, #15).
fn report_ecam(sim: &Sim) {
    let ecam = sim.read_ecam();
    if ecam.is_empty() {
        println!("                 ECAM: clear");
        return;
    }
    for w in ecam {
        println!(
            "                 ECAM: {} {} ({})",
            w.severity.as_str().to_uppercase(),
            w.message,
            w.source.as_str()
        );
    }
}

fn main() {
    let mut sim = Sim::new();

    // Cold & dark: los pulsadores leen su default OFF (sin seeding, D-007).
    sim.step(1000);
    report("[cold & dark]", &sim.get(OBSERVED).unwrap());

    // Baterías ON: el DC BAT bus cobra vida (solo baterías, sin AC).
    sim.set("OVHD_ELEC_BAT_1_PB_IS_AUTO", 1.0).unwrap();
    sim.set("OVHD_ELEC_BAT_2_PB_IS_AUTO", 1.0).unwrap();
    sim.run(2.0, 5.0);
    report("[baterias ON]", &sim.get(OBSERVED).unwrap());

    // Ext pwr conectada y ON: la red AC completa se alimenta. El bus tie debe
    // estar en AUTO para que ext pwr alimente los buses AC (sin seeding arranca
    // en 0; ver D-007 y el test de integración).
    sim.set("OVHD_ELEC_BUS_TIE_PB_IS_AUTO", 1.0).unwrap();
    sim.set("EXT_PWR_AVAIL:1", 1.0).unwrap();
    sim.set("OVHD_ELEC_EXT_PWR_PB_IS_ON", 1.0).unwrap();
    sim.run(2.0, 5.0);
    report("[ext pwr ON]", &sim.get(OBSERVED).unwrap());

    // Fallo del TR 1 por id estable (Fase 2, #14). Lo interesante no es que algo
    // se apague, sino que la red se **reconfigura**: el DC 1 se sigue
    // alimentando por el bus tie, como en el avión real, mientras el TR 1 deja
    // de dar potencial normal.
    sim.inject_failure("elec.tr.1").unwrap();
    sim.run(2.0, 5.0);
    report("[fail TR 1]", &sim.get(OBSERVED).unwrap());
    report_ecam(&sim);

    // Y limpiarlo devuelve el TR a su estado normal y retira la caution.
    sim.clear_failure("elec.tr.1").unwrap();
    sim.run(2.0, 5.0);
    report("[unfail TR 1]", &sim.get(OBSERVED).unwrap());
    report_ecam(&sim);

    demo_apu_gen_fault();
}

/// Demo del criterio de la Fase 2 (#16): tirar un generador y ver su caution.
///
/// Avión aparte, y **sin ext pwr** a propósito: la condición del fault del APU
/// GEN exige `!ext_pwr_contactor_closed()`. Las baterías sí hacen falta — el
/// motor de arranque del APU cuelga de sus contactores.
fn demo_apu_gen_fault() {
    println!("\n--- Fase 2 (#16): el fallo de un generador levanta su caution ---");
    let mut sim = Sim::new();

    // El Rust de FBW no quema combustible, solo lee la cantidad: con
    // `UNLIMITED FUEL` el APU arranca sin modelar el sistema de fuel (Fase 4).
    sim.set("UNLIMITED FUEL", 1.0).unwrap();
    sim.set("bat_1", 1.0).unwrap();
    sim.set("bat_2", 1.0).unwrap();
    sim.run(3.0, 5.0);

    sim.set("OVHD_APU_MASTER_SW_PB_IS_ON", 1.0).unwrap();
    sim.run(1.0, 5.0);
    sim.set("OVHD_APU_START_PB_IS_ON", 1.0).unwrap();

    // Espera acotada a la turbina (~62 s de simulación).
    let mut elapsed = 0;
    while sim.get(&["OVHD_APU_START_PB_IS_AVAILABLE"]).unwrap()["OVHD_APU_START_PB_IS_AVAILABLE"]
        == 0.0
    {
        sim.run(1.0, 10.0);
        elapsed += 1;
        assert!(elapsed < 150, "el APU no llegó a available");
    }
    println!("[APU arrancado]  disponible a t={:.0}s", sim.sim_time());

    sim.set("apu_gen", 1.0).unwrap();
    sim.set("bus_tie", 1.0).unwrap();
    sim.run(5.0, 5.0);
    println!("[APU GEN ON]     la red AC la alimenta el APU");
    report_ecam(&sim);

    sim.inject_failure("elec.apu_gen.1").unwrap();
    sim.run(5.0, 5.0);
    println!("[fail APU GEN]   se cae la unica fuente AC");
    report_ecam(&sim);

    sim.clear_failure("elec.apu_gen.1").unwrap();
    sim.run(5.0, 5.0);
    println!("[unfail APU GEN] la red se recupera");
    report_ecam(&sim);
}
