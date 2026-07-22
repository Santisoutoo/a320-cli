//! Test de integración: **el fallo de un generador levanta su caution** (#16).
//!
//! Es el criterio de éxito de la Fase 2 de `CLAUDE.md` ("tirar un generador y ver
//! la caution correspondiente aparecer"), y la primera vez que las dos mitades se
//! encuentran: inyección (#14) y detección (#15). Si esto pasa, el entorno ya
//! puede plantearle un problema a un agente: algo se rompe y el avión lo dice.
//!
//! ## Por qué el APU GEN y no un generador de motor
//!
//! El arranque de motores es de Fase 4, así que `Generator(1)`/`Generator(2)` no
//! son ejercitables todavía: sin motor girando, su contactor está abierto de
//! todos modos y el fault no distinguiría un fallo de un estado normal.
//!
//! El APU sí es arrancable en tierra, y su fault es además el **único** flag
//! eléctrico correctamente gateado por el estado real del sistema
//! (`apu.is_available()`, `a320_systems/src/electrical/mod.rs:306`), así que no
//! da falsos positivos. Es el mejor caso disponible, y satisface el criterio al
//! pie de la letra: es un generador de verdad.
//!
//! El arranque no arrastra un modelo de consumo (el Rust de FBW no quema
//! combustible, solo lee cantidades — `fuel/mod.rs:1`, *"Fuel system for now is
//! still handled in MSFS"*): el APU bebe del tanque left main, que el runtime
//! siembra con la carga por defecto (slice 3 de Fase 4, #57) — la antigua
//! muleta `UNLIMITED FUEL` ya no hace falta.

use a320_sim_core::api::Sim;
use a320_sim_core::ecam::{EcamSource, Severity, Warning};
use a320_sim_core::failures::FailureGroup;

const APU_GEN_FAULT: &str = "elec.apu_gen.fault";
const AC_ESS_FAULT: &str = "elec.ac_ess_feed.fault";
const APU_AVAILABLE: &str = "OVHD_APU_START_PB_IS_AVAILABLE";

const AC_BUSES: &[&str] = &[
    "ELEC_AC_1_BUS_IS_POWERED",
    "ELEC_AC_2_BUS_IS_POWERED",
    "ELEC_AC_ESS_BUS_IS_POWERED",
];

/// Cota superior del arranque del APU, en segundos de simulación. El APS3200
/// tarda ~46 s en llegar a N>95% (`apu/aps3200.rs:167`, `TIME_LIMIT: 45.12`) más
/// 2 s sostenidos para declararse disponible (`electronic_control_box.rs:328`);
/// medido, llega a los ~62 s. 150 s da margen de sobra sin colgar el CI si algo
/// cambiara upstream: el test falla con un mensaje claro en vez de girar para
/// siempre.
const APU_START_TIMEOUT_S: u32 = 150;

fn find<'a>(ecam: &'a [Warning], id: &str) -> Option<&'a Warning> {
    ecam.iter().find(|w| w.id == id)
}

/// Arranca el APU y lo deja alimentando la red AC entera.
///
/// **Sin ext pwr a propósito**: la condición del fault del APU GEN exige
/// `!ext_pwr_contactor_closed()`. Las baterías, en cambio, son obligatorias — no
/// por conveniencia, sino porque el motor de arranque del APU cuelga de los dos
/// contactores de batería (`electrical/direct_current.rs:206`:
/// `close_when(battery_1_contactor.is_closed() && battery_2_contactor.is_closed() && ...)`).
fn apu_powering_the_network() -> Sim {
    let mut sim = Sim::new();

    // (0) La bomba hidráulica amarilla es un pulsador AUTO/ON invertido y sin
    //     seeding (D-007) lee 0 = ON: con la red AC viva arrancaría y metería
    //     transitorios hidráulicos (LO PR / fault de bomba) en un escenario
    //     eléctrico. Se aparca en AUTO (su posición real de cold & dark).
    sim.set("hyd_epump_yellow", 1.0).unwrap();

    // (1) Baterías dentro (para el motor de arranque). El combustible viene
    //     del seed por defecto del runtime (#57).
    sim.set("bat_1", 1.0).unwrap();
    sim.set("bat_2", 1.0).unwrap();
    sim.run(3.0, 5.0);

    // (2) APU MASTER SW y START, por nombre amigable (catalogados desde el
    //     slice 2 de Fase 4, #56 — mismo cambio que en el arnés del MCP).
    sim.set("apu_master", 1.0).unwrap();
    sim.run(1.0, 5.0);
    sim.set("apu_start", 1.0).unwrap();

    // (3) Espera **acotada** a que la turbina esté disponible (no un sleep a
    //     ciegas: se afirma que llega, y en cuánto).
    let mut elapsed = 0;
    while sim.get(&[APU_AVAILABLE]).unwrap()[APU_AVAILABLE] == 0.0 {
        sim.run(1.0, 10.0);
        elapsed += 1;
        assert!(
            elapsed < APU_START_TIMEOUT_S,
            "el APU no llegó a available en {APU_START_TIMEOUT_S} s de simulación"
        );
    }

    // (4) APU GEN dentro: alimenta la red AC vía los bus tie contactors.
    sim.set("apu_gen", 1.0).unwrap();
    sim.set("bus_tie", 1.0).unwrap();
    sim.run(5.0, 5.0);
    sim
}

#[test]
fn apu_generator_failure_raises_its_caution_and_clearing_retires_it() {
    let mut sim = apu_powering_the_network();

    // --- (1) precondición: el APU alimenta la red y la ECAM está limpia ------
    let s = sim.get(AC_BUSES).unwrap();
    for &bus in AC_BUSES {
        assert!(
            s[bus] != 0.0,
            "precondición: {bus} debería estar alimentado por el APU GEN"
        );
    }
    // Con el APU sano no hay cautions — pero la ECAM no está vacía: desde el
    // slice 2 de Fase 4 (#56) el memo verde APU AVAIL acompaña al APU en marcha.
    let ecam = sim.read_ecam();
    assert!(
        ecam.iter().all(|w| w.severity == Severity::Advisory),
        "precondición: con el APU sano no debe haber cautions, ECAM: {ecam:?}"
    );
    assert!(
        find(&ecam, "apu.avail").is_some(),
        "precondición: con el APU en marcha se espera el memo APU AVAIL, ECAM: {ecam:?}"
    );

    // --- (2) tirar el generador ---------------------------------------------
    sim.inject_failure("elec.apu_gen.1").unwrap();
    sim.run(5.0, 5.0);

    let ecam = sim.read_ecam();
    let w = find(&ecam, APU_GEN_FAULT)
        .unwrap_or_else(|| panic!("se esperaba la caution del APU GEN, ECAM: {ecam:?}"));

    // El criterio del issue es explícito: no basta el texto. Severidad y sistema
    // también tienen que ser correctos, porque es de lo que razona el agente.
    assert_eq!(w.message, "APU GEN FAULT");
    assert_eq!(w.severity, Severity::Caution);
    assert_eq!(w.system, FailureGroup::Elec);
    // Y esta caution la calcula FBW, no nosotros: es su modelo diciendo que hay
    // un fault, no una regla nuestra. Es la señal más fuerte que da este test.
    assert_eq!(w.source, EcamSource::VendorFlag);

    // La consecuencia aguas abajo, que es lo que hace realista al escenario: al
    // caer la única fuente AC, la red AC se queda muerta y el AC ESS levanta su
    // propia caution. Un agente tiene que lidiar con las dos.
    let s = sim.get(AC_BUSES).unwrap();
    for &bus in AC_BUSES {
        assert!(
            s[bus] == 0.0,
            "tras el fallo: {bus} debería quedarse sin alimentar (era la única fuente AC)"
        );
    }
    assert!(
        find(&ecam, AC_ESS_FAULT).is_some(),
        "tras el fallo: se esperaba también la caution del AC ESS, ECAM: {ecam:?}"
    );

    // La ECAM sigue legible porque las baterías mantienen vivo el DC ESS: por eso
    // el gate de alimentación mira AC ESS **o** DC ESS. Si mirase solo el AC, este
    // escenario —perder toda la red AC— se quedaría mudo justo cuando más importa.
    let s = sim.get(&["ELEC_DC_ESS_BUS_IS_POWERED"]).unwrap();
    assert_eq!(
        s["ELEC_DC_ESS_BUS_IS_POWERED"], 1.0,
        "el DC ESS debe seguir vivo (baterías): es lo que mantiene la ECAM legible"
    );

    // --- (3) limpiar el fallo retira las cautions ----------------------------
    sim.clear_failure("elec.apu_gen.1").unwrap();
    sim.run(5.0, 5.0);

    // Las cautions se retiran; el memo APU AVAIL (advisory) permanece, porque
    // el APU sigue en marcha — reparar el generador no apaga la turbina.
    let ecam = sim.read_ecam();
    assert!(
        ecam.iter().all(|w| w.severity == Severity::Advisory),
        "fallo limpiado: las cautions deberían retirarse, ECAM: {ecam:?}"
    );
    let s = sim.get(AC_BUSES).unwrap();
    for &bus in AC_BUSES {
        assert!(s[bus] != 0.0, "fallo limpiado: {bus} debería recuperarse");
    }
}
