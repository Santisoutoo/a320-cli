//! `read_ecam()`: warnings/cautions activos, para que el agente observe.
//!
//! ## Por qué esto es un motor de reglas y no un mapeo del FWC
//!
//! `CLAUDE.md` anticipaba "mapear los warnings del FWC". **No hay FWC en el Rust
//! vendorizado**: búsqueda exhaustiva de `flight_warning`/`FlightWarningComputer`/
//! `master_caution` en todo el árbol (`fbw-a32nx`, `fbw-a380x`, `fbw-common`) da
//! cero coincidencias. El propio vendor lo reconoce
//! (`a320_systems/src/surveillance.rs:73`: *"TODO: Comes from FWC"*). El texto de
//! los mensajes ECAM vive en la capa TypeScript, que ni compilamos ni está
//! vendorizada (el submódulo está en sparse-checkout, solo `src/wasm`).
//!
//! Consecuencia: **el catálogo de mensajes es nuestro**, derivado de las
//! variables que el Rust sí escribe. Ver `docs/fase2-ecam.md` y D-014 en
//! `docs/decisiones.md`.
//!
//! ## `EcamSource`: de quién es la lógica
//!
//! Cada regla declara si el disparador lo calcula FBW ([`EcamSource::VendorFlag`])
//! o si la regla es nuestra ([`EcamSource::Derived`]). No es cosmético: es la
//! frontera entre el ground truth heredado y el inventado. La contribución de
//! investigación es el entorno evaluable; si en la Fase 5 no se puede decir qué
//! parte del ground truth es de FBW, no se puede decir qué mide el benchmark.
//!
//! ## El gate de alimentación
//!
//! Sin FWC no hay lógica de inhibición, y algunos flags de FBW están crudos: el
//! de AC ESS FEED es `!ac_ess_bus_is_powered` **sin más condiciones**, así que en
//! cold & dark vale `true` sin ningún fallo inyectado (verificado: el propio test
//! de FBW `when_ac_ess_bus_is_unpowered_ac_ess_feed_has_fault` lo afirma). Un
//! mapeo naive daría una caution en un avión sano.
//!
//! Por eso las reglas solo se evalúan si la ECAM estaría **viva**. No es un
//! parche para pasar el test: en el avión real la ECAM no está alimentada en cold
//! & dark — no hay nada que mostrar. El criterio del issue #15 y la fidelidad al
//! avión piden lo mismo.

use crate::failures::FailureGroup;
use crate::variables::VariableStore;

/// Severidad de un mensaje ECAM, en orden de prioridad descendente.
///
/// El orden del enum **es** el orden de prioridad (`Ord` derivado): es lo que
/// usa `read_ecam()` para ordenar, así que un `Warning` siempre precede a una
/// `Caution`. En el avión real esto es rojo (+ master warning) vs ámbar
/// (+ master caution) vs verde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Rojo: exige acción inmediata (master warning).
    Warning,
    /// Ámbar: exige atención (master caution).
    Caution,
    /// Verde/memo: informativo.
    Advisory,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Warning => "warning",
            Severity::Caution => "caution",
            Severity::Advisory => "advisory",
        }
    }
}

/// Procedencia de la lógica de una regla: ¿la calcula FBW o la inventamos?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcamSource {
    /// El disparador es un flag que **calcula el modelo de FBW** (p. ej. la luz
    /// FAULT de un pulsador del overhead). Alta confianza: es el avión de FBW
    /// diciendo que hay un fault.
    VendorFlag,
    /// La regla es **nuestra**, derivada de estado que FBW expone. FBW no dice
    /// "hay un fault" aquí; lo concluimos nosotros a partir de sus variables.
    Derived,
}

impl EcamSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            EcamSource::VendorFlag => "vendor_flag",
            EcamSource::Derived => "derived",
        }
    }
}

/// Una condición elemental sobre el store (para los gates compuestos).
#[derive(Debug, Clone, Copy)]
enum Cond {
    /// El LVAR es distinto de 0.
    Set(&'static str),
    /// El LVAR es exactamente 0 (para pulsadores de semántica invertida, como
    /// el AUTO/ON de la bomba amarilla: 0 = ON).
    Clear(&'static str),
    /// El LVAR supera un umbral (para "el circuito aguas arriba está
    /// presurizado", que no es booleano).
    Above(&'static str, f64),
}

impl Cond {
    fn holds(&self, store: &VariableStore) -> bool {
        match self {
            Cond::Set(lvar) => is_set(store, lvar),
            Cond::Clear(lvar) => !is_set(store, lvar),
            Cond::Above(lvar, threshold) => store.peek_by_name(lvar) > *threshold,
        }
    }

    fn lvar(&self) -> &'static str {
        match self {
            Cond::Set(lvar) | Cond::Clear(lvar) | Cond::Above(lvar, _) => lvar,
        }
    }
}

/// Cómo se decide si una regla está activa.
#[derive(Debug, Clone, Copy)]
enum Trigger {
    /// Un flag booleano que escribe FBW: activo si el LVAR es distinto de 0.
    Flag(&'static str),
    /// TR sin potencial normal **mientras su bus AC de entrada está vivo**.
    ///
    /// La condición de "AC vivo" no es decorativa: sin ella, un TR sin
    /// alimentar (cold & dark, o AC caído por otra causa) se reportaría como
    /// averiado. Un TR sin AC no está roto, está apagado — el mensaje sería
    /// falso y, peor, taparía la causa real.
    TrFaultWithAcAlive {
        potential_normal: &'static str,
        ac_bus_powered: &'static str,
    },
    /// Presión de circuito por debajo de un umbral **mientras la fuente de
    /// presión del circuito está comandada** (todas las `enable` se cumplen).
    ///
    /// Mismo motivo que el gate de los TR: en cold & dark las tres presiones
    /// hidráulicas valen 0 sin que nada esté roto — un circuito sin bomba
    /// comandada no está averiado, está apagado. Solo cuando algo *debería*
    /// estar presurizando (bomba eléctrica ON con AC vivo, PTU en AUTO con el
    /// otro circuito presurizado) una presión baja es un LO PR de verdad.
    PressureBelowWhileAll {
        pressure: &'static str,
        threshold_psi: f64,
        enable: &'static [Cond],
    },
}

/// Una regla del catálogo ECAM.
#[derive(Debug, Clone, Copy)]
pub struct EcamRule {
    /// Id estable de la regla (`elec.apu_gen.fault`).
    pub id: &'static str,
    /// Texto tal como lo mostraría la ECAM.
    pub message: &'static str,
    pub severity: Severity,
    /// Sistema al que pertenece (reutiliza el grupo del catálogo de fallos).
    pub system: FailureGroup,
    /// ¿La lógica es de FBW o nuestra?
    pub source: EcamSource,
    trigger: Trigger,
}

/// Un warning activo, tal como lo ve el agente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Warning {
    pub id: &'static str,
    pub message: &'static str,
    pub severity: Severity,
    pub system: FailureGroup,
    pub source: EcamSource,
}

/// Buses que alimentan la ECAM. Si ninguno está vivo, no hay pantalla que mirar.
const ECAM_POWER_BUSES: &[&str] = &["ELEC_AC_ESS_BUS_IS_POWERED", "ELEC_DC_ESS_BUS_IS_POWERED"];

/// Catálogo de reglas. **Fase 2: eléctrico. Fase 4 slice 1: hidráulico. Solo lo
/// alcanzable.**
///
/// Queda deliberadamente fuera el RAT & EMER GEN FAULT
/// (`OVHD_EMER_ELEC_RAT_AND_EMER_GEN_PB_HAS_FAULT`): su condición exige
/// `!context.is_on_ground()` (`a320_systems/src/electrical/mod.rs:408`) y toda la
/// Fase 2 transcurre en tierra. No se cataloga lo que ningún test puede levantar.
///
/// Tampoco hay regla de BAT FAULT: las baterías **nunca** reciben `set_fault` en
/// FBW, así que `OVHD_ELEC_BAT_x_PB_HAS_FAULT` es siempre 0. No está modelado.
pub const CATALOG: &[EcamRule] = &[
    EcamRule {
        id: "elec.apu_gen.fault",
        message: "APU GEN FAULT",
        severity: Severity::Caution,
        system: FailureGroup::Elec,
        source: EcamSource::VendorFlag,
        trigger: Trigger::Flag("OVHD_ELEC_APU_GEN_PB_HAS_FAULT"),
    },
    EcamRule {
        id: "elec.eng_gen.1.fault",
        message: "ENG 1 GEN FAULT",
        severity: Severity::Caution,
        system: FailureGroup::Elec,
        source: EcamSource::VendorFlag,
        trigger: Trigger::Flag("OVHD_ELEC_ENG_GEN_1_PB_HAS_FAULT"),
    },
    EcamRule {
        id: "elec.eng_gen.2.fault",
        message: "ENG 2 GEN FAULT",
        severity: Severity::Caution,
        system: FailureGroup::Elec,
        source: EcamSource::VendorFlag,
        trigger: Trigger::Flag("OVHD_ELEC_ENG_GEN_2_PB_HAS_FAULT"),
    },
    EcamRule {
        id: "elec.ac_ess_feed.fault",
        message: "AC ESS BUS FAULT",
        severity: Severity::Caution,
        system: FailureGroup::Elec,
        source: EcamSource::VendorFlag,
        trigger: Trigger::Flag("OVHD_ELEC_AC_ESS_FEED_PB_HAS_FAULT"),
    },
    // Los TR no tienen luz de fault en el overhead (ni en el avión real ni en
    // FBW): no hay flag que leer, así que la regla es NUESTRA.
    EcamRule {
        id: "elec.tr.1.fault",
        message: "ELEC TR 1 FAULT",
        severity: Severity::Caution,
        system: FailureGroup::Elec,
        source: EcamSource::Derived,
        trigger: Trigger::TrFaultWithAcAlive {
            potential_normal: "ELEC_TR_1_POTENTIAL_NORMAL",
            ac_bus_powered: "ELEC_AC_1_BUS_IS_POWERED",
        },
    },
    EcamRule {
        id: "elec.tr.2.fault",
        message: "ELEC TR 2 FAULT",
        severity: Severity::Caution,
        system: FailureGroup::Elec,
        source: EcamSource::Derived,
        trigger: Trigger::TrFaultWithAcAlive {
            potential_normal: "ELEC_TR_2_POTENTIAL_NORMAL",
            ac_bus_powered: "ELEC_AC_2_BUS_IS_POWERED",
        },
    },
    // --- Hidráulico (Fase 4, slice 1) ----------------------------------------
    //
    // Faults de bomba: la luz FAULT del pulsador la calcula FBW (agrega presión
    // baja, presión de aire del reservorio baja, nivel bajo y overheat), así que
    // son VendorFlag. El mensaje es genérico ("... PUMP FAULT") a propósito: el
    // flag no distingue la causa.
    EcamRule {
        id: "hyd.eng_1_pump.fault",
        message: "HYD ENG 1 PUMP FAULT",
        severity: Severity::Caution,
        system: FailureGroup::Hyd,
        source: EcamSource::VendorFlag,
        trigger: Trigger::Flag("OVHD_HYD_ENG_1_PUMP_PB_HAS_FAULT"),
    },
    EcamRule {
        id: "hyd.eng_2_pump.fault",
        message: "HYD ENG 2 PUMP FAULT",
        severity: Severity::Caution,
        system: FailureGroup::Hyd,
        source: EcamSource::VendorFlag,
        trigger: Trigger::Flag("OVHD_HYD_ENG_2_PUMP_PB_HAS_FAULT"),
    },
    EcamRule {
        id: "hyd.epump_b.fault",
        message: "HYD B ELEC PUMP FAULT",
        severity: Severity::Caution,
        system: FailureGroup::Hyd,
        source: EcamSource::VendorFlag,
        trigger: Trigger::Flag("OVHD_HYD_EPUMPB_PB_HAS_FAULT"),
    },
    EcamRule {
        id: "hyd.epump_y.fault",
        message: "HYD Y ELEC PUMP FAULT",
        severity: Severity::Caution,
        system: FailureGroup::Hyd,
        source: EcamSource::VendorFlag,
        trigger: Trigger::Flag("OVHD_HYD_EPUMPY_PB_HAS_FAULT"),
    },
    // LO PR derivados: FBW no expone un flag "SYS LO PR" a nivel de circuito,
    // así que la regla es nuestra sobre la presión de la system section, con el
    // gate de "fuente comandada" (ver `Trigger::PressureBelowWhileAll`). El
    // umbral de 1450 psi es el del propio vendor (histéresis baja del low press
    // switch, hydraulic/mod.rs:3262). **Alcance slice 1 (sin motores)**: las
    // fuentes catalogadas son las alcanzables en tierra — bombas eléctricas y
    // PTU; los caminos por EDP llegan con los motores (slice 4).
    EcamRule {
        id: "hyd.g.lo_pr",
        message: "HYD G SYS LO PR",
        severity: Severity::Caution,
        system: FailureGroup::Hyd,
        source: EcamSource::Derived,
        trigger: Trigger::PressureBelowWhileAll {
            pressure: "HYD_GREEN_SYSTEM_1_SECTION_PRESSURE",
            threshold_psi: 1450.0,
            // Sin motores, lo único que presuriza el verde es el PTU: comandado
            // en AUTO y con el amarillo presurizado del que transferir.
            enable: &[
                Cond::Set("OVHD_HYD_PTU_PB_IS_AUTO"),
                Cond::Above("HYD_YELLOW_SYSTEM_1_SECTION_PRESSURE", 1450.0),
            ],
        },
    },
    EcamRule {
        id: "hyd.b.lo_pr",
        message: "HYD B SYS LO PR",
        severity: Severity::Caution,
        system: FailureGroup::Hyd,
        source: EcamSource::Derived,
        trigger: Trigger::PressureBelowWhileAll {
            pressure: "HYD_BLUE_SYSTEM_1_SECTION_PRESSURE",
            threshold_psi: 1450.0,
            // En tierra la bomba azul solo bombea con el pulsador en AUTO, el
            // override del panel de mantenimiento activo y su bus AC 1 vivo
            // (hydraulic/mod.rs:3134-3142; el camino de vuelo/motores es slice 4).
            enable: &[
                Cond::Set("OVHD_HYD_EPUMPB_PB_IS_AUTO"),
                Cond::Set("OVHD_HYD_EPUMPY_OVRD_IS_ON"),
                Cond::Set("ELEC_AC_1_BUS_IS_POWERED"),
            ],
        },
    },
    EcamRule {
        id: "hyd.y.lo_pr",
        message: "HYD Y SYS LO PR",
        severity: Severity::Caution,
        system: FailureGroup::Hyd,
        source: EcamSource::Derived,
        trigger: Trigger::PressureBelowWhileAll {
            pressure: "HYD_YELLOW_SYSTEM_1_SECTION_PRESSURE",
            threshold_psi: 1450.0,
            // La bomba amarilla es AUTO/ON invertida: comandada cuando el
            // pulsador NO está en AUTO (0 = ON) y su bus de alimentación (AC
            // GND/FLT SVC) está vivo (hydraulic/mod.rs:3310-3312, :1597).
            enable: &[
                Cond::Clear("OVHD_HYD_EPUMPY_PB_IS_AUTO"),
                Cond::Set("ELEC_AC_GND_FLT_SVC_BUS_IS_POWERED"),
            ],
        },
    },
    // Memo del PTU: la lógica es del vendor entera ("Actual logic of HYD PTU
    // memo computed here until done within FWS", hydraulic/mod.rs:2609-2641).
    EcamRule {
        id: "hyd.ptu.memo",
        message: "HYD PTU",
        severity: Severity::Advisory,
        system: FailureGroup::Hyd,
        source: EcamSource::VendorFlag,
        trigger: Trigger::Flag("HYD_PTU_ON_ECAM_MEMO"),
    },
    // --- APU (Fase 4, slice 2) ------------------------------------------------
    //
    // El wording "APU FAULT" (y no "APU MASTER FAULT") es el de la caution ECAM
    // real del A320: la luz FAULT del MASTER SW acompaña a un auto-shutdown del
    // APU, y lo que la E/WD muestra es "APU FAULT". El flag lo calcula FBW: el
    // ECB propaga su fault al pulsador (`fbw-common/.../systems/src/apu/mod.rs:371`,
    // `self.master.set_fault(apu.has_fault())`) y hoy lo levantan dos causas —
    // pérdida de presión de combustible con el APU girando
    // (`electronic_control_box.rs:224-230`, `ApuFault::FuelLowPressure`) y el
    // fire button soltado (`electronic_control_box.rs:150-152`, `ApuFault::ApuFire`).
    EcamRule {
        id: "apu.master.fault",
        message: "APU FAULT",
        severity: Severity::Caution,
        system: FailureGroup::Apu,
        source: EcamSource::VendorFlag,
        trigger: Trigger::Flag("OVHD_APU_MASTER_SW_PB_HAS_FAULT"),
    },
    // Memo verde AVAIL: el ECB declara el APU disponible con N>95% sostenido 2 s
    // (o N>99.5%) y sin fault ni cooldown (`electronic_control_box.rs:328-337`);
    // el pulsador START lo refleja en su LVAR (`apu/mod.rs:361`,
    // `self.start.set_available(apu.is_available())`).
    EcamRule {
        id: "apu.avail",
        message: "APU AVAIL",
        severity: Severity::Advisory,
        system: FailureGroup::Apu,
        source: EcamSource::VendorFlag,
        trigger: Trigger::Flag("OVHD_APU_START_PB_IS_AVAILABLE"),
    },
];

fn is_set(store: &VariableStore, name: &str) -> bool {
    store.peek_by_name(name) != 0.0
}

/// ¿Está la ECAM alimentada? Ver el gate en la doc del módulo.
fn ecam_is_powered(store: &VariableStore) -> bool {
    ECAM_POWER_BUSES.iter().any(|bus| is_set(store, bus))
}

impl EcamRule {
    fn is_active(&self, store: &VariableStore) -> bool {
        match self.trigger {
            Trigger::Flag(lvar) => is_set(store, lvar),
            Trigger::TrFaultWithAcAlive {
                potential_normal,
                ac_bus_powered,
            } => is_set(store, ac_bus_powered) && !is_set(store, potential_normal),
            Trigger::PressureBelowWhileAll {
                pressure,
                threshold_psi,
                enable,
            } => {
                store.peek_by_name(pressure) < threshold_psi
                    && enable.iter().all(|cond| cond.holds(store))
            }
        }
    }

    /// LVARs que lee la regla (para el test anti-drift).
    pub fn read_lvars(&self) -> Vec<&'static str> {
        match self.trigger {
            Trigger::Flag(lvar) => vec![lvar],
            Trigger::TrFaultWithAcAlive {
                potential_normal,
                ac_bus_powered,
            } => vec![potential_normal, ac_bus_powered],
            Trigger::PressureBelowWhileAll {
                pressure, enable, ..
            } => std::iter::once(pressure)
                .chain(enable.iter().map(Cond::lvar))
                .collect(),
        }
    }
}

/// Evalúa el catálogo contra el estado actual y devuelve los warnings activos,
/// ordenados por severidad (y por id, para que el orden sea determinista).
pub fn read(store: &VariableStore) -> Vec<Warning> {
    if !ecam_is_powered(store) {
        // ECAM sin alimentar: no hay pantalla que leer. Ver el gate del módulo.
        return Vec::new();
    }

    let mut out: Vec<Warning> = CATALOG
        .iter()
        .filter(|rule| rule.is_active(store))
        .map(|rule| Warning {
            id: rule.id,
            message: rule.message,
            severity: rule.severity,
            system: rule.system,
            source: rule.source,
        })
        .collect();
    out.sort_by(|a, b| a.severity.cmp(&b.severity).then(a.id.cmp(b.id)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_ids_and_messages_are_unique() {
        for (i, a) in CATALOG.iter().enumerate() {
            for b in &CATALOG[i + 1..] {
                assert_ne!(a.id, b.id, "id de regla duplicado: {}", a.id);
                assert_ne!(a.message, b.message, "mensaje duplicado: {}", a.message);
            }
        }
    }

    #[test]
    fn severity_orders_warning_before_caution_before_advisory() {
        assert!(Severity::Warning < Severity::Caution);
        assert!(Severity::Caution < Severity::Advisory);
    }

    /// El prefijo del id y el sistema declarado deben coincidir (mismo motivo
    /// que en el catálogo de fallos: el agrupado de CLI/MCP depende de ello).
    #[test]
    fn rule_id_prefix_matches_the_declared_system() {
        for r in CATALOG {
            let expected_prefix = match r.system {
                FailureGroup::Elec => "elec.",
                FailureGroup::Hyd => "hyd.",
                FailureGroup::Apu => "apu.",
            };
            assert!(
                r.id.starts_with(expected_prefix),
                "la regla '{}' declara sistema {:?} pero su id no empieza por '{expected_prefix}'",
                r.id,
                r.system
            );
        }
    }

    #[test]
    fn hydraulic_rules_cover_the_phase4_slice() {
        for id in [
            "hyd.g.lo_pr",
            "hyd.b.lo_pr",
            "hyd.y.lo_pr",
            "hyd.eng_1_pump.fault",
            "hyd.eng_2_pump.fault",
            "hyd.epump_b.fault",
            "hyd.epump_y.fault",
            "hyd.ptu.memo",
        ] {
            assert!(
                CATALOG.iter().any(|r| r.id == id),
                "falta la regla ECAM '{id}'"
            );
        }
        // El memo es informativo, no una caution.
        let memo = CATALOG.iter().find(|r| r.id == "hyd.ptu.memo").unwrap();
        assert_eq!(memo.severity, Severity::Advisory);
    }

    #[test]
    fn apu_rules_cover_the_phase4_slice() {
        // Las dos reglas que el issue #56 (slice 2) exige para el APU.
        let fault = CATALOG.iter().find(|r| r.id == "apu.master.fault").unwrap();
        assert_eq!(fault.severity, Severity::Caution);
        assert_eq!(fault.source, EcamSource::VendorFlag);

        let avail = CATALOG.iter().find(|r| r.id == "apu.avail").unwrap();
        assert_eq!(avail.severity, Severity::Advisory);
        assert_eq!(avail.source, EcamSource::VendorFlag);
    }

    /// El RAT & EMER GEN queda fuera a propósito: su condición exige
    /// `!is_on_ground()` y la Fase 2 es en tierra. Si alguien lo añade sin
    /// levantar el escenario de vuelo, este test le recuerda por qué no estaba.
    #[test]
    fn no_rule_depends_on_being_airborne() {
        assert!(
            !CATALOG.iter().any(|r| r.id.contains("rat_and_emer")),
            "el RAT/EMER GEN fault exige !is_on_ground(): no es alcanzable en Fase 2"
        );
    }
}
