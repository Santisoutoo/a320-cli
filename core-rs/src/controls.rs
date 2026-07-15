//! Catálogo **curado a mano** de controles del A320.
//!
//! `list_variables()` (registro) vuelca cientos de nombres crudos de LVAR: es
//! una herramienta de depuración, no una interfaz. Este catálogo es la otra
//! mitad — las cosas que un piloto puede *accionar* de verdad, con un nombre
//! amigable que un humano adivina y un LLM recibe, más los metadatos que un
//! esquema necesita (tipo, valores válidos, descripción, grupo). Lo consumen
//! los dos frontends: la CLI para autocompletar, el MCP para el esquema de
//! `set_control`.
//!
//! **Curado, no generado**: el objetivo es que *una persona* haya decidido que
//! estos son los controles reales. Un volcado del registro obligaría al LLM a
//! adivinar cuáles de cientos de variables son switches escribibles.
//!
//! **Alcance de Fase 1**: el panel eléctrico (baterías, ext pwr, APU gen, bus
//! tie, generadores). Se amplía por fase.
//!
//! **Cabina vs mundo**: `OVHD_ELEC_BAT_1_PB_IS_AUTO` es un pulsador de cabina;
//! `EXT_PWR_AVAIL:1` es estado de "mundo" que en un sim real vendría de MSFS y
//! aquí falsificamos (si hay un GPU enchufado). Ambos los escribe el frontend,
//! solo uno es un control de cabina: se distinguen con [`ControlDomain`].

/// Tipo de dato de un control (issue #10: bool/enum/float).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    /// Booleano (pulsador/switch de dos estados).
    Bool,
    /// Selector discreto de un conjunto de posiciones.
    Enum,
    /// Magnitud continua (knob).
    Float,
}

impl ControlKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ControlKind::Bool => "bool",
            ControlKind::Enum => "enum",
            ControlKind::Float => "float",
        }
    }
}

/// Dominio del control: cabina real vs mundo exterior falsificado.
///
/// La distinción importa para el frontend: un control de cabina es algo que el
/// piloto/agente acciona como parte del procedimiento; un fake de mundo es un
/// estado del entorno que en un sim real no tocaría (lo pondría MSFS), y que
/// aquí exponemos solo para poder montar escenarios (p. ej. "hay GPU").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlDomain {
    /// Pulsador/knob real de cabina que un piloto acciona.
    Cockpit,
    /// Estado de "mundo" que falsificamos (lo daría MSFS en un sim real).
    World,
}

impl ControlDomain {
    pub fn as_str(&self) -> &'static str {
        match self {
            ControlDomain::Cockpit => "cockpit",
            ControlDomain::World => "world",
        }
    }
}

/// Sistema al que pertenece el control (para agrupar en CLI/MCP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlGroup {
    /// Sistema eléctrico.
    Elec,
    // Hyd, Apu, Pneu, Fuel... se añaden en fases posteriores.
}

impl ControlGroup {
    pub fn as_str(&self) -> &'static str {
        match self {
            ControlGroup::Elec => "ELEC",
        }
    }
}

/// Valores admisibles de un control.
///
/// Es la fuente de verdad de la validación de [`crate::api::Sim::set`]: un valor
/// que no pase [`ValidValues::check`] se rechaza con un error accionable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValidValues {
    /// Booleano: solo `0.0` (off/false) o `1.0` (on/true).
    Bool,
    /// Conjunto discreto de valores válidos (para selectores).
    Enum(&'static [f64]),
    /// Rango continuo inclusivo `[min, max]`.
    Range { min: f64, max: f64 },
}

impl ValidValues {
    /// ¿Es `value` admisible? Devuelve `Ok(())` o un motivo legible para el
    /// error (el REPL y el LLM necesitan saber *por qué* se rechazó).
    pub fn check(&self, value: f64) -> Result<(), String> {
        match self {
            ValidValues::Bool => {
                if value == 0.0 || value == 1.0 {
                    Ok(())
                } else {
                    Err("must be 0 (off) or 1 (on)".to_owned())
                }
            }
            ValidValues::Enum(allowed) => {
                if allowed.contains(&value) {
                    Ok(())
                } else {
                    Err(format!("must be one of {allowed:?}"))
                }
            }
            ValidValues::Range { min, max } => {
                if value >= *min && value <= *max {
                    Ok(())
                } else {
                    Err(format!("must be within [{min}, {max}]"))
                }
            }
        }
    }

    /// Descripción legible de los valores válidos (para esquemas y ayuda).
    pub fn describe(&self) -> String {
        match self {
            ValidValues::Bool => "0 (off) or 1 (on)".to_owned(),
            ValidValues::Enum(allowed) => format!("one of {allowed:?}"),
            ValidValues::Range { min, max } => format!("[{min}, {max}]"),
        }
    }
}

/// Una entrada del catálogo: un control accionable con sus metadatos.
///
/// `Copy` porque todos los campos son `Copy` (`&'static str` + enums): el
/// catálogo es estático y las entradas se pasan por valor sin coste.
#[derive(Debug, Clone, Copy)]
pub struct Control {
    /// Nombre amigable y estable (lo que se teclea / se le da al LLM).
    pub name: &'static str,
    /// LVAR subyacente que se escribe realmente en el registro.
    pub lvar: &'static str,
    /// Tipo de dato (bool/enum/float).
    pub kind: ControlKind,
    /// Valores admisibles.
    pub valid: ValidValues,
    /// Descripción de una línea.
    pub description: &'static str,
    /// Grupo por sistema.
    pub group: ControlGroup,
    /// Cabina real vs fake de mundo.
    pub domain: ControlDomain,
}

/// Catálogo curado. **Fase 1: panel eléctrico.**
///
/// Los LVAR provienen del panel eléctrico superior de FBW
/// (`a320_systems/src/electrical/mod.rs`, `A320ElectricalOverheadPanel::new`) y
/// del `ExternalPowerSource` (`systems/.../external_power_source.rs`). Los
/// sufijos `_PB_IS_AUTO` / `_PB_IS_ON` los fija el tipo de pulsador de FBW:
/// `AutoOffFaultPushButton` usa AUTO, `OnOffFaultPushButton`/`OnOffAvailable`
/// usan ON. El test `every_catalog_lvar_is_registered_after_a_tick` verifica
/// que cada `lvar` de aquí existe en el registro (caza typos y drift del vendor).
pub const CATALOG: &[Control] = &[
    Control {
        name: "bat_1",
        lvar: "OVHD_ELEC_BAT_1_PB_IS_AUTO",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Battery 1 master pushbutton: 1 = AUTO (battery in the loop), 0 = OFF",
        group: ControlGroup::Elec,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "bat_2",
        lvar: "OVHD_ELEC_BAT_2_PB_IS_AUTO",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Battery 2 master pushbutton: 1 = AUTO (battery in the loop), 0 = OFF",
        group: ControlGroup::Elec,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "ext_pwr",
        lvar: "OVHD_ELEC_EXT_PWR_PB_IS_ON",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "External power pushbutton: 1 = ON (GPU feeds the AC network), 0 = OFF",
        group: ControlGroup::Elec,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "apu_gen",
        lvar: "OVHD_ELEC_APU_GEN_PB_IS_ON",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "APU generator pushbutton: 1 = ON (APU gen supplies the network), 0 = OFF",
        group: ControlGroup::Elec,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "bus_tie",
        lvar: "OVHD_ELEC_BUS_TIE_PB_IS_AUTO",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Bus tie pushbutton: 1 = AUTO (buses tie as needed), 0 = OFF (buses isolated)",
        group: ControlGroup::Elec,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "gen_1",
        lvar: "OVHD_ELEC_ENG_GEN_1_PB_IS_ON",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Engine 1 generator pushbutton: 1 = ON, 0 = OFF",
        group: ControlGroup::Elec,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "gen_2",
        lvar: "OVHD_ELEC_ENG_GEN_2_PB_IS_ON",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Engine 2 generator pushbutton: 1 = ON, 0 = OFF",
        group: ControlGroup::Elec,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "ext_pwr_avail",
        lvar: "EXT_PWR_AVAIL:1",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "External power availability (world state we fake): 1 = GPU plugged in, 0 = not connected",
        group: ControlGroup::Elec,
        domain: ControlDomain::World,
    },
];

/// Busca una entrada por su nombre amigable.
pub fn by_name(name: &str) -> Option<&'static Control> {
    CATALOG.iter().find(|c| c.name == name)
}

/// Busca una entrada por su LVAR subyacente.
pub fn by_lvar(lvar: &str) -> Option<&'static Control> {
    CATALOG.iter().find(|c| c.lvar == lvar)
}

/// Resuelve un identificador de control (nombre amigable **o** LVAR) a su
/// entrada del catálogo. Aceptar el LVAR además del nombre amigable mantiene
/// compatible el camino de escritura por LVAR crudo de la Fase 1 (issue #9).
pub fn resolve(control: &str) -> Option<&'static Control> {
    by_name(control).or_else(|| by_lvar(control))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_the_phase1_electrical_panel() {
        // Los controles que el issue #10 exige para Fase 1.
        for name in [
            "bat_1", "bat_2", "ext_pwr", "apu_gen", "bus_tie", "gen_1", "gen_2",
        ] {
            assert!(by_name(name).is_some(), "falta el control '{name}'");
        }
    }

    #[test]
    fn friendly_names_are_unique() {
        for (i, a) in CATALOG.iter().enumerate() {
            for b in &CATALOG[i + 1..] {
                assert_ne!(a.name, b.name, "nombre amigable duplicado: {}", a.name);
            }
        }
    }

    #[test]
    fn lvars_are_unique() {
        for (i, a) in CATALOG.iter().enumerate() {
            for b in &CATALOG[i + 1..] {
                assert_ne!(a.lvar, b.lvar, "LVAR duplicado: {}", a.lvar);
            }
        }
    }

    #[test]
    fn cockpit_controls_are_distinguished_from_world_fakes() {
        // El pulsador de ext pwr es cabina; la disponibilidad de GPU es mundo.
        assert_eq!(by_name("ext_pwr").unwrap().domain, ControlDomain::Cockpit);
        assert_eq!(
            by_name("ext_pwr_avail").unwrap().domain,
            ControlDomain::World
        );
        // El único fake de mundo del catálogo de Fase 1 es EXT_PWR_AVAIL:1.
        let world: Vec<_> = CATALOG
            .iter()
            .filter(|c| c.domain == ControlDomain::World)
            .map(|c| c.name)
            .collect();
        assert_eq!(world, vec!["ext_pwr_avail"]);
    }

    #[test]
    fn resolve_accepts_both_friendly_name_and_lvar() {
        let by_friendly = resolve("bat_1").unwrap();
        let by_raw = resolve("OVHD_ELEC_BAT_1_PB_IS_AUTO").unwrap();
        assert_eq!(by_friendly.lvar, by_raw.lvar);
        assert_eq!(by_friendly.name, "bat_1");
        assert!(resolve("no_such_control").is_none());
    }

    #[test]
    fn bool_validation_accepts_only_zero_and_one() {
        let v = ValidValues::Bool;
        assert!(v.check(0.0).is_ok());
        assert!(v.check(1.0).is_ok());
        assert!(v.check(0.5).is_err());
        assert!(v.check(2.0).is_err());
        assert!(v.check(-1.0).is_err());
    }

    #[test]
    fn range_validation_is_inclusive() {
        let v = ValidValues::Range {
            min: 0.0,
            max: 10.0,
        };
        assert!(v.check(0.0).is_ok());
        assert!(v.check(10.0).is_ok());
        assert!(v.check(5.0).is_ok());
        assert!(v.check(-0.1).is_err());
        assert!(v.check(10.1).is_err());
    }

    #[test]
    fn enum_validation_matches_the_allowed_set() {
        let v = ValidValues::Enum(&[1.0, 2.0, 3.0]);
        assert!(v.check(2.0).is_ok());
        assert!(v.check(4.0).is_err());
    }
}
