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
    println!(
        "                 fallos activos: {:?}",
        sim.active_failures()
    );

    // Y limpiarlo devuelve el TR a su estado normal.
    sim.clear_failure("elec.tr.1").unwrap();
    sim.run(2.0, 5.0);
    report("[unfail TR 1]", &sim.get(OBSERVED).unwrap());
}
