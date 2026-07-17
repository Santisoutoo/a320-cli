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

/// Catálogo de reglas. **Fase 2: eléctrico, y solo lo alcanzable.**
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

    #[test]
    fn phase2_rules_are_all_electrical() {
        assert!(CATALOG.iter().all(|r| r.system == FailureGroup::Elec));
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
