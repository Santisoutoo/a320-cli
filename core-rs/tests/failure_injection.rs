//! Test de integración de la inyección de fallos (issue #14).
//!
//! Reproduce **por la API pública** el fallo que el spike de Fase 0 hacía a mano
//! con `bed.fail(FailureType::TransformerRectifier(1))` sobre el test bed de
//! FBW. Es el criterio literal del issue: lo que antes exigía tocar el enum del
//! vendor ahora se pide con un id estable nuestro (`elec.tr.1`).
//!
//! El caso del TR 1 es deliberado: no es una pérdida plana de energía, sino una
//! **reconfiguración de red** — el DC 1 se sigue alimentando por el bus tie,
//! como en el avión real, mientras el TR 1 deja de dar potencial normal. Un
//! mapeo mal hecho del id al `FailureType` podría apagar algo y "parecer" que
//! funciona; exigir la reconfiguración correcta es lo que prueba que el id llega
//! exactamente al componente que toca.

use std::collections::BTreeMap;

use a320_sim_core::api::Sim;

// --- Controles ---------------------------------------------------------------
const BAT_1_PB_IS_AUTO: &str = "OVHD_ELEC_BAT_1_PB_IS_AUTO";
const BAT_2_PB_IS_AUTO: &str = "OVHD_ELEC_BAT_2_PB_IS_AUTO";
const EXT_PWR_AVAIL: &str = "EXT_PWR_AVAIL:1";
const EXT_PWR_PB_IS_ON: &str = "OVHD_ELEC_EXT_PWR_PB_IS_ON";
const BUS_TIE_PB_IS_AUTO: &str = "OVHD_ELEC_BUS_TIE_PB_IS_AUTO";

// --- Outputs -----------------------------------------------------------------
const AC_1_BUS: &str = "ELEC_AC_1_BUS_IS_POWERED";
const DC_1_BUS: &str = "ELEC_DC_1_BUS_IS_POWERED";
const DC_2_BUS: &str = "ELEC_DC_2_BUS_IS_POWERED";
const TR_1_POTENTIAL_NORMAL: &str = "ELEC_TR_1_POTENTIAL_NORMAL";
const TR_2_POTENTIAL_NORMAL: &str = "ELEC_TR_2_POTENTIAL_NORMAL";

const OBSERVED: &[&str] = &[
    AC_1_BUS,
    DC_1_BUS,
    DC_2_BUS,
    TR_1_POTENTIAL_NORMAL,
    TR_2_POTENTIAL_NORMAL,
];

fn settle(sim: &mut Sim) {
    sim.run(2.0, 5.0);
}

fn powered(state: &BTreeMap<String, f64>, name: &str) -> bool {
    state[name] != 0.0
}

/// Lleva el avión a red AC completa con external power (el estado final del
/// test del slice eléctrico, que aquí es solo el punto de partida).
fn powered_network() -> Sim {
    let mut sim = Sim::new();
    sim.set(BAT_1_PB_IS_AUTO, 1.0).unwrap();
    sim.set(BAT_2_PB_IS_AUTO, 1.0).unwrap();
    sim.set(BUS_TIE_PB_IS_AUTO, 1.0).unwrap();
    sim.set(EXT_PWR_AVAIL, 1.0).unwrap();
    sim.set(EXT_PWR_PB_IS_ON, 1.0).unwrap();
    settle(&mut sim);
    sim
}

#[test]
fn tr_1_failure_reconfigures_the_network_through_the_public_api() {
    let mut sim = powered_network();

    // --- (1) red sana: TR 1 y TR 2 dando potencial normal --------------------
    let s = sim.get(OBSERVED).unwrap();
    assert!(powered(&s, AC_1_BUS), "precondición: AC 1 alimentado");
    assert!(
        powered(&s, TR_1_POTENTIAL_NORMAL),
        "precondición: TR 1 con potencial normal"
    );
    assert!(powered(&s, DC_1_BUS), "precondición: DC 1 alimentado");
    assert!(sim.active_failures().is_empty(), "precondición: sin fallos");

    // --- (2) fallo del TR 1 por id estable -----------------------------------
    sim.inject_failure("elec.tr.1").unwrap();
    settle(&mut sim);

    let s = sim.get(OBSERVED).unwrap();
    assert_eq!(sim.active_failures(), vec!["elec.tr.1"]);
    assert!(
        !powered(&s, TR_1_POTENTIAL_NORMAL),
        "TR 1 fallado: no debería dar potencial normal"
    );
    // Lo que hace este caso valioso: la red se reconfigura en vez de apagarse.
    // El DC 1 se realimenta por el bus tie desde el TR 2, como en el avión real.
    assert!(
        powered(&s, DC_1_BUS),
        "TR 1 fallado: DC 1 debería seguir alimentado vía bus tie (reconfiguración)"
    );
    assert!(
        powered(&s, TR_2_POTENTIAL_NORMAL),
        "TR 1 fallado: el TR 2 no debería verse afectado"
    );
    assert!(
        powered(&s, DC_2_BUS),
        "TR 1 fallado: DC 2 no debería verse afectado"
    );

    // --- (3) limpiar el fallo devuelve el sistema a su estado previo ---------
    sim.clear_failure("elec.tr.1").unwrap();
    settle(&mut sim);

    let s = sim.get(OBSERVED).unwrap();
    assert!(sim.active_failures().is_empty());
    assert!(
        powered(&s, TR_1_POTENTIAL_NORMAL),
        "fallo limpiado: TR 1 debería volver a dar potencial normal"
    );
    assert!(powered(&s, DC_1_BUS), "fallo limpiado: DC 1 alimentado");
}

/// Criterio del issue: inyectar y limpiar devuelve el sistema al estado previo.
///
/// Se barre el snapshot **entero** (no una lista de variables elegida a dedo,
/// que solo probaría lo que ya se sabe) y se exige la vuelta de todo el **estado
/// discreto** de la red: qué está alimentado y qué da potencial normal.
///
/// Las magnitudes **continuas** (`ELEC_*_CURRENT`, `_POTENTIAL`, `_LOAD`) quedan
/// deliberadamente fuera y no es una concesión para pasar el test: son estado
/// físico integrado en el tiempo. La batería se descarga un poco mientras el TR
/// está fallado, así que su corriente de carga **no debe** volver al mismo float
/// — exigirlo sería exigir que el avión olvide que el fallo ocurrió. Lo que sí
/// tiene que volver es la topología: los mismos buses vivos y los mismos TRs en
/// normal. (Descubierto al escribir el test: `ELEC_BAT_1_CURRENT` no volvía.)
#[test]
fn injecting_then_clearing_restores_the_networks_discrete_state() {
    let mut sim = powered_network();
    let before = sim.snapshot();

    sim.inject_failure("elec.tr.1").unwrap();
    settle(&mut sim);
    let during = sim.snapshot();
    assert_ne!(before, during, "el fallo debe cambiar el estado del avión");

    sim.clear_failure("elec.tr.1").unwrap();
    settle(&mut sim);
    let after = sim.snapshot();

    let is_discrete_state =
        |name: &str| name.ends_with("_IS_POWERED") || name.ends_with("_POTENTIAL_NORMAL");

    let mut checked = 0;
    for (name, value) in &before {
        if name.starts_with("ELEC_") && is_discrete_state(name) {
            assert_eq!(
                after.get(name),
                Some(value),
                "'{name}' no volvió a su valor previo tras limpiar el fallo"
            );
            checked += 1;
        }
    }
    // Que el barrido no se quede vacío por un cambio de nomenclatura upstream:
    // un test que no comprueba nada pasa siempre.
    assert!(
        checked > 5,
        "se esperaban varias variables de estado discreto, se comprobaron {checked}"
    );
}
