//! Capa de control/observación: **una** API, dos frontends.
//!
//! Es la capa limpia del diagrama de `CLAUDE.md`. Tanto la CLI como el servidor
//! MCP son ventanas a esto y no debe conocer a ninguno: aquí no hay formas JSON,
//! esquemas de tools, prompts ni formateo de terminal. Acertar en esta frontera
//! es justo el motivo de construirla una sola vez.
//!
//! Contrato (subconjunto de Fase 1, `CLAUDE.md`):
//! - [`Sim::set`] — actuar un control (switch/pulsador/knob).
//! - [`Sim::get`] — leer estado de sistemas.
//! - [`Sim::step`] / [`Sim::run`] — avanzar el tiempo.
//! - [`Sim::set_environment`] — el mundo exterior (issue #8).
//! - [`Sim::snapshot`] — volcado completo del estado.
//! - [`Sim::list_variables`] — descubrimiento crudo (todo el registro).
//! - [`Sim::list_controls`] — descubrimiento curado (catálogo de controles, #10).
//!
//! `read_ecam()` y la inyección/limpieza de fallos son de **Fase 2** (#14, #15):
//! se les deja sitio (los errores y la fachada no cierran la puerta), pero no se
//! stubbean aquí.

use std::collections::BTreeMap;
use std::fmt;

use systems::simulation::StartState;

use crate::controls::{self, Control};
use crate::runtime::Runtime;

/// Errores de la API, tipados y con mensaje útil: un REPL y un LLM necesitan
/// saber *qué* estuvo mal, no solo que algo falló.
#[derive(Debug, Clone, PartialEq)]
pub enum ApiError {
    /// Nombre de control/variable que el avión no conoce (típicamente un typo).
    /// Usa [`Sim::list_variables`] para descubrir los nombres válidos.
    UnknownControl { name: String },
    /// Valor no admisible para un control (p. ej. NaN o infinito).
    BadValue {
        name: String,
        value: f64,
        reason: String,
    },
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::UnknownControl { name } => write!(
                f,
                "unknown control '{name}' (not a known variable; use list_variables() to discover valid names)"
            ),
            ApiError::BadValue {
                name,
                value,
                reason,
            } => write!(f, "bad value {value} for control '{name}': {reason}"),
        }
    }
}

impl std::error::Error for ApiError {}

/// Fachada de la API sobre el runtime persistente.
///
/// Encapsula un [`Runtime`] y expone el contrato limpio. No filtra tipos de CLI
/// ni de MCP.
pub struct Sim {
    runtime: Runtime,
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

impl Sim {
    /// Crea un avión en cold & dark en el apron (perfil por defecto de Fase 1).
    pub fn new() -> Self {
        Self {
            runtime: Runtime::apron_cold_and_dark(),
        }
    }

    /// Crea un avión en un `StartState` concreto.
    pub fn with_start_state(start_state: StartState) -> Self {
        Self {
            runtime: Runtime::new(start_state),
        }
    }

    /// ¿Es `name` un control/variable conocido por el avión?
    ///
    /// El registro, tras construir el avión, contiene todos los nombres que los
    /// sistemas leen/escriben más los del entorno: es el catálogo de nombres
    /// válidos. La comprobación no acuña identificadores (no muta el registro).
    fn is_known(&self, name: &str) -> bool {
        self.runtime.store().registry.find(name).is_some()
    }

    /// Actúa un control escribiendo su variable de entrada.
    ///
    /// `control` puede ser un **nombre amigable** del catálogo (p. ej. `bat_1`)
    /// o el **LVAR** subyacente (`OVHD_ELEC_BAT_1_PB_IS_AUTO`); ambos resuelven a
    /// la misma entrada. La resolución es:
    ///
    /// 1. Si está en el catálogo curado ([`crate::controls`]), el valor se
    ///    valida contra sus valores válidos (bool/enum/rango) y se escribe en su
    ///    LVAR. Un valor fuera de rango se rechaza con [`ApiError::BadValue`].
    /// 2. Si no, se acepta como variable cruda del registro (compatibilidad con
    ///    la Fase 1, #9): solo se valida que sea finita y que el nombre exista.
    ///
    /// Errores: [`ApiError::BadValue`] si el valor no es finito o está fuera de
    /// los valores válidos del control; [`ApiError::UnknownControl`] si el nombre
    /// no es ni un control del catálogo ni una variable conocida (no se acuña un
    /// identificador nuevo en ese caso).
    pub fn set(&mut self, control: &str, value: f64) -> Result<(), ApiError> {
        if !value.is_finite() {
            return Err(ApiError::BadValue {
                name: control.to_owned(),
                value,
                reason: "value must be finite (not NaN or infinity)".to_owned(),
            });
        }

        // 1. Control del catálogo (por nombre amigable o por LVAR): validación
        //    de rango y escritura en el LVAR curado.
        if let Some(entry) = controls::resolve(control) {
            if let Err(reason) = entry.valid.check(value) {
                return Err(ApiError::BadValue {
                    name: control.to_owned(),
                    value,
                    reason,
                });
            }
            self.runtime.write_by_name(entry.lvar, value);
            return Ok(());
        }

        // 2. Variable cruda no catalogada: validación mínima contra el registro.
        if !self.is_known(control) {
            return Err(ApiError::UnknownControl {
                name: control.to_owned(),
            });
        }
        self.runtime.write_by_name(control, value);
        Ok(())
    }

    /// Lee el estado de las variables pedidas.
    ///
    /// Devuelve un mapa nombre→valor. Error [`ApiError::UnknownControl`] en la
    /// primera variable desconocida (no acuña identificadores).
    pub fn get(&self, vars: &[&str]) -> Result<BTreeMap<String, f64>, ApiError> {
        let mut out = BTreeMap::new();
        for &name in vars {
            if !self.is_known(name) {
                return Err(ApiError::UnknownControl {
                    name: name.to_owned(),
                });
            }
            out.insert(name.to_owned(), self.runtime.store().peek_by_name(name));
        }
        Ok(out)
    }

    /// Avanza la simulación `dt_ms` milisegundos en un solo tick.
    pub fn step(&mut self, dt_ms: u64) {
        self.runtime.step(dt_ms);
    }

    /// Avanza `seconds` segundos ejecutando ticks a `rate` Hz.
    pub fn run(&mut self, seconds: f64, rate: f64) {
        self.runtime.run(seconds, rate);
    }

    /// Fija el mundo exterior con los knobs de alto nivel (issue #8).
    pub fn set_environment(
        &mut self,
        altitude_ft: f64,
        indicated_airspeed_kt: f64,
        oat_celsius: f64,
        qnh_hpa: f64,
    ) {
        self.runtime
            .set_environment(altitude_ft, indicated_airspeed_kt, oat_celsius, qnh_hpa);
    }

    /// Volcado completo del estado: todas las variables conocidas y su valor.
    pub fn snapshot(&self) -> BTreeMap<String, f64> {
        self.runtime.store().snapshot()
    }

    /// Nombres de todas las variables conocidas (para descubrimiento crudo).
    pub fn list_variables(&self) -> Vec<String> {
        self.runtime.store().list_variables()
    }

    /// Catálogo curado de controles accionables (issue #10).
    ///
    /// A diferencia de [`Sim::list_variables`] (que vuelca el registro entero),
    /// esto devuelve las entradas curadas a mano — nombre amigable, LVAR, tipo,
    /// valores válidos, descripción, grupo y dominio (cabina/mundo). Es lo que
    /// la CLI usa para autocompletar y el MCP para el esquema de `set_control`.
    pub fn list_controls(&self) -> Vec<Control> {
        controls::CATALOG.to_vec()
    }

    /// Tiempo de simulación acumulado en segundos (monótono).
    pub fn sim_time(&self) -> f64 {
        self.runtime.sim_time()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAT_1: &str = "OVHD_ELEC_BAT_1_PB_IS_AUTO";
    const BAT_2: &str = "OVHD_ELEC_BAT_2_PB_IS_AUTO";
    const DC_BAT_BUS: &str = "ELEC_DC_BAT_BUS_IS_POWERED";
    const AC_1_BUS: &str = "ELEC_AC_1_BUS_IS_POWERED";

    #[test]
    fn cold_and_dark_to_battery_on_through_the_api_alone() {
        let mut sim = Sim::new();

        // Cold & dark: toda la red sin alimentar.
        sim.step(1000);
        let s = sim.get(&[DC_BAT_BUS, AC_1_BUS]).unwrap();
        assert_eq!(s[DC_BAT_BUS], 0.0, "DC BAT off en cold & dark");
        assert_eq!(s[AC_1_BUS], 0.0, "AC 1 off en cold & dark");

        // Baterías ON solo por la API.
        sim.set(BAT_1, 1.0).unwrap();
        sim.set(BAT_2, 1.0).unwrap();
        sim.run(2.0, 5.0); // settling

        let s = sim.get(&[DC_BAT_BUS, AC_1_BUS]).unwrap();
        assert_eq!(s[DC_BAT_BUS], 1.0, "DC BAT ON con baterías");
        assert_eq!(s[AC_1_BUS], 0.0, "AC 1 sigue off (sin fuente AC)");
    }

    #[test]
    fn set_unknown_control_is_a_typed_error() {
        let mut sim = Sim::new();
        let err = sim.set("OVHD_ELEC_BAT_1_PB_IS_ATUO", 1.0).unwrap_err();
        match err {
            ApiError::UnknownControl { name } => assert_eq!(name, "OVHD_ELEC_BAT_1_PB_IS_ATUO"),
            other => panic!("esperaba UnknownControl, fue {other:?}"),
        }
        // No debe haber acuñado la variable con typo.
        assert!(!sim
            .list_variables()
            .iter()
            .any(|n| n == "OVHD_ELEC_BAT_1_PB_IS_ATUO"));
    }

    #[test]
    fn set_non_finite_value_is_a_typed_error() {
        let mut sim = Sim::new();
        let err = sim.set(BAT_1, f64::NAN).unwrap_err();
        assert!(matches!(err, ApiError::BadValue { .. }));
        // El mensaje debe ser útil.
        assert!(err.to_string().contains("finite"));
    }

    #[test]
    fn get_unknown_variable_is_a_typed_error() {
        let sim = Sim::new();
        let err = sim.get(&["NO SUCH VAR"]).unwrap_err();
        assert!(matches!(err, ApiError::UnknownControl { .. }));
    }

    #[test]
    fn snapshot_and_list_variables_expose_the_registry() {
        let mut sim = Sim::new();
        sim.step(200);

        let vars = sim.list_variables();
        assert!(vars.iter().any(|n| n == DC_BAT_BUS));

        let snap = sim.snapshot();
        assert_eq!(snap.len(), vars.len(), "snapshot cubre todas las variables");
        assert!(snap.contains_key(DC_BAT_BUS));
    }

    #[test]
    fn every_catalog_lvar_is_registered_after_a_tick() {
        // El test clave del issue #10: cada LVAR del catálogo debe existir en el
        // registro tras un tick. Caza typos en el catálogo y drift del vendor
        // (un LVAR renombrado upstream deja de registrarse y esto lo detecta).
        let mut sim = Sim::new();
        sim.step(1000);
        let vars = sim.list_variables();
        for c in controls::CATALOG {
            assert!(
                vars.iter().any(|n| n == c.lvar),
                "LVAR '{}' del control '{}' no está en el registro tras un tick",
                c.lvar,
                c.name
            );
        }
    }

    #[test]
    fn list_controls_exposes_the_curated_catalog() {
        let sim = Sim::new();
        let controls = sim.list_controls();
        assert!(!controls.is_empty());
        assert!(controls.iter().any(|c| c.name == "bat_1"));
        // Todos los de Fase 1 son del grupo eléctrico.
        assert!(controls
            .iter()
            .all(|c| c.group == crate::controls::ControlGroup::Elec));
    }

    #[test]
    fn set_by_friendly_name_writes_the_underlying_lvar() {
        let mut sim = Sim::new();
        // Misma secuencia probada que el test por LVAR crudo, pero accionando por
        // nombre amigable: debe escribir el LVAR de FBW y encender el DC BAT bus.
        sim.step(1000);
        sim.set("bat_1", 1.0).unwrap();
        sim.set("bat_2", 1.0).unwrap();
        sim.run(2.0, 5.0);

        // El nombre amigable escribe exactamente el LVAR subyacente.
        let raw = sim.get(&["OVHD_ELEC_BAT_1_PB_IS_AUTO"]).unwrap();
        assert_eq!(raw["OVHD_ELEC_BAT_1_PB_IS_AUTO"], 1.0);

        let s = sim.get(&[DC_BAT_BUS]).unwrap();
        assert_eq!(
            s[DC_BAT_BUS], 1.0,
            "DC BAT ON con baterías por nombre amigable"
        );
    }

    #[test]
    fn set_out_of_range_value_is_rejected_with_a_useful_error() {
        let mut sim = Sim::new();
        // Un pulsador booleano solo admite 0/1: 5.0 debe rechazarse.
        let err = sim.set("bat_1", 5.0).unwrap_err();
        match err {
            ApiError::BadValue {
                name,
                value,
                reason,
            } => {
                assert_eq!(name, "bat_1");
                assert_eq!(value, 5.0);
                assert!(
                    reason.contains("0") && reason.contains("1"),
                    "motivo útil: {reason}"
                );
            }
            other => panic!("esperaba BadValue, fue {other:?}"),
        }
        // El rechazo también aplica por LVAR crudo del catálogo.
        assert!(sim.set(BAT_1, 2.0).is_err());
    }

    #[test]
    fn set_environment_through_the_api() {
        let mut sim = Sim::new();
        sim.set_environment(1000.0, 0.0, 5.0, 1013.25);
        sim.step(1000);

        let s = sim.get(&["SIM ON GROUND", "PRESSURE ALTITUDE"]).unwrap();
        assert_eq!(s["SIM ON GROUND"], 1.0);
        assert!((s["PRESSURE ALTITUDE"] - 1000.0).abs() < 1e-6);
    }
}
