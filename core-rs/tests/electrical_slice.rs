//! Test de integración del vertical slice eléctrico (issue #13).
//!
//! Es el criterio de éxito de la Fase 1 (CLAUDE.md) convertido en test de
//! regresión: cold & dark -> baterías ON -> ext pwr, con los buses cobrando
//! vida. A diferencia del spike (`main.rs`, que imprimía a stdout y lo validaba
//! un humano) y del test unitario de `api.rs` (que solo cubría DC BAT / AC 1),
//! aquí se recorre la máquina de estados **entera por la API pública**
//! (`a320_sim_core::api::Sim`) y se llega hasta la **red AC completa** vía
//! external power: buses AC, TRs con potencial normal y DC 1 alimentado.
//!
//! Que sea un test (y no una demo) es lo que atrapa un cambio de comportamiento
//! si un bump del pin del vendor moviera la lógica eléctrica por debajo: la
//! premisa del benchmark es que el modelo del avión es reproducible.
//!
//! Corre nativo con `cargo test` y es rápido (apto para CI): instancia el avión
//! una vez y avanza unos pocos segundos de settling por transición.

use std::collections::BTreeMap;

use a320_sim_core::api::Sim;

// --- Controles (inputs que escribe el operador) -----------------------------
const BAT_1_PB_IS_AUTO: &str = "OVHD_ELEC_BAT_1_PB_IS_AUTO";
const BAT_2_PB_IS_AUTO: &str = "OVHD_ELEC_BAT_2_PB_IS_AUTO";
const EXT_PWR_AVAIL: &str = "EXT_PWR_AVAIL:1";
const EXT_PWR_PB_IS_ON: &str = "OVHD_ELEC_EXT_PWR_PB_IS_ON";
const BUS_TIE_PB_IS_AUTO: &str = "OVHD_ELEC_BUS_TIE_PB_IS_AUTO";

// --- Buses y TRs (outputs que escriben los sistemas) ------------------------
const AC_1_BUS: &str = "ELEC_AC_1_BUS_IS_POWERED";
const AC_2_BUS: &str = "ELEC_AC_2_BUS_IS_POWERED";
const DC_1_BUS: &str = "ELEC_DC_1_BUS_IS_POWERED";
const DC_2_BUS: &str = "ELEC_DC_2_BUS_IS_POWERED";
const DC_BAT_BUS: &str = "ELEC_DC_BAT_BUS_IS_POWERED";
const DC_HOT_1_BUS: &str = "ELEC_DC_HOT_1_BUS_IS_POWERED";
const TR_1_POTENTIAL_NORMAL: &str = "ELEC_TR_1_POTENTIAL_NORMAL";
const TR_2_POTENTIAL_NORMAL: &str = "ELEC_TR_2_POTENTIAL_NORMAL";

/// Buses de la **red de distribución**: alimentados vía contactores que
/// dependen de las fuentes (baterías / AC). En cold & dark deben estar todos
/// muertos. NO incluye el DC HOT 1, que cuelga directamente de la batería
/// (aguas arriba del contactor del pulsador) y por tanto está siempre vivo.
const NETWORK_BUSES: &[&str] = &[
    AC_1_BUS,
    AC_2_BUS,
    DC_1_BUS,
    DC_2_BUS,
    DC_BAT_BUS,
    TR_1_POTENTIAL_NORMAL,
    TR_2_POTENTIAL_NORMAL,
];

/// Superconjunto observado en cada paso (red + hot bus).
const OBSERVED: &[&str] = &[
    AC_1_BUS,
    AC_2_BUS,
    DC_1_BUS,
    DC_2_BUS,
    DC_BAT_BUS,
    DC_HOT_1_BUS,
    TR_1_POTENTIAL_NORMAL,
    TR_2_POTENTIAL_NORMAL,
];

/// Settling entre transiciones: 2 s a 5 Hz (10 ticks de 200 ms), el patrón del
/// spike. Suficiente para que la red eléctrica se reconfigure y estabilice.
fn settle(sim: &mut Sim) {
    sim.run(2.0, 5.0);
}

fn powered(state: &BTreeMap<String, f64>, name: &str) -> bool {
    state[name] != 0.0
}

#[test]
fn electrical_slice_cold_and_dark_to_battery_to_ext_pwr() {
    let mut sim = Sim::new();

    // --- (1) cold & dark: red de distribución sin alimentar ------------------
    // Sin seeding (D-007), los pulsadores leen su default OFF; el spike tenía
    // que forzar las baterías a OFF a mano, aquí ya se obtiene de serie.
    sim.step(1000);
    let s = sim.get(OBSERVED).unwrap();
    for &name in NETWORK_BUSES {
        assert!(
            !powered(&s, name),
            "cold & dark: {name} debería estar sin alimentar, fue {}",
            s[name]
        );
    }
    // El DC HOT 1 cuelga directo de la batería 1 (electrical/direct_current.rs:
    // `flow(hot_bus_1, battery_1)`), aguas arriba del contactor del pulsador:
    // está vivo aun en cold & dark con las baterías en OFF. Es el hot bus real.
    assert!(
        powered(&s, DC_HOT_1_BUS),
        "cold & dark: DC HOT 1 debería seguir vivo (cuelga directo de la batería)"
    );

    // --- (2) BAT 1+2 ON: DC BAT bus con energía, AC sigue muerta -------------
    sim.set(BAT_1_PB_IS_AUTO, 1.0).unwrap();
    sim.set(BAT_2_PB_IS_AUTO, 1.0).unwrap();
    settle(&mut sim);

    let s = sim.get(OBSERVED).unwrap();
    assert!(
        powered(&s, DC_BAT_BUS),
        "baterías ON: DC BAT bus debería estar alimentado"
    );
    // Solo baterías: ninguna fuente AC, así que la red AC (y sus TRs, y los
    // buses DC alimentados vía TR) sigue muerta.
    assert!(
        !powered(&s, AC_1_BUS),
        "baterías ON: AC 1 debería seguir muerto (sin fuente AC)"
    );
    assert!(
        !powered(&s, AC_2_BUS),
        "baterías ON: AC 2 debería seguir muerto (sin fuente AC)"
    );
    assert!(
        !powered(&s, TR_1_POTENTIAL_NORMAL),
        "baterías ON: TR 1 sin potencial normal (sin AC)"
    );
    assert!(
        !powered(&s, DC_1_BUS),
        "baterías ON: DC 1 sin alimentar (se alimenta vía TR 1)"
    );
    assert!(powered(&s, DC_HOT_1_BUS), "baterías ON: DC HOT 1 sigue vivo");

    // --- (3) EXT PWR disponible + ON: red AC completa alimentada -------------
    // `EXT_PWR_AVAIL:1` = fuente conectada en el GPU; el pulsador ON la mete en
    // la red. Con AC vivo, los TRs dan potencial normal y los buses DC se
    // alimentan vía TR.
    //
    // El bus tie DEBE estar en AUTO: los buses AC 1/2 se alimentan desde ext pwr
    // únicamente a través de los bus tie contactors, que cierran solo si
    // `bus_tie_is_auto()` (electrical/alternating_current.rs). En el avión real
    // el bus tie está normalmente en AUTO; en nuestro cold & dark sin seeding
    // (D-007) el pulsador `new_auto` arranca en 0, así que hay que ponerlo por
    // nombre — la vía prescrita por D-007, no parchear el vendor.
    sim.set(BUS_TIE_PB_IS_AUTO, 1.0).unwrap();
    sim.set(EXT_PWR_AVAIL, 1.0).unwrap();
    sim.set(EXT_PWR_PB_IS_ON, 1.0).unwrap();
    settle(&mut sim);

    let s = sim.get(OBSERVED).unwrap();
    assert!(
        powered(&s, AC_1_BUS),
        "ext pwr ON: AC 1 debería estar alimentado"
    );
    assert!(
        powered(&s, AC_2_BUS),
        "ext pwr ON: AC 2 debería estar alimentado"
    );
    assert!(
        powered(&s, TR_1_POTENTIAL_NORMAL),
        "ext pwr ON: TR 1 debería dar potencial normal"
    );
    assert!(
        powered(&s, TR_2_POTENTIAL_NORMAL),
        "ext pwr ON: TR 2 debería dar potencial normal"
    );
    assert!(
        powered(&s, DC_1_BUS),
        "ext pwr ON: DC 1 debería estar alimentado (vía TR 1)"
    );
    assert!(
        powered(&s, DC_2_BUS),
        "ext pwr ON: DC 2 debería estar alimentado (vía TR 2)"
    );
    // El DC BAT bus sigue vivo (ahora servido por la red, no solo baterías).
    assert!(
        powered(&s, DC_BAT_BUS),
        "ext pwr ON: DC BAT bus debería seguir alimentado"
    );
    assert!(powered(&s, DC_HOT_1_BUS), "ext pwr ON: DC HOT 1 sigue vivo");
}
