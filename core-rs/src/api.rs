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
//! - [`Sim::list_variables`] — descubrimiento.
//!
//! `read_ecam()` y la inyección/limpieza de fallos son de **Fase 2** (#14, #15):
//! se les deja sitio (los errores y la fachada no cierran la puerta), pero no se
//! stubbean aquí.

use std::collections::BTreeMap;
use std::fmt;

use systems::simulation::StartState;

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
    /// Errores: [`ApiError::BadValue`] si el valor no es finito;
    /// [`ApiError::UnknownControl`] si el nombre no está en el catálogo (no se
    /// acuña un identificador nuevo en ese caso).
    pub fn set(&mut self, control: &str, value: f64) -> Result<(), ApiError> {
        if !value.is_finite() {
            return Err(ApiError::BadValue {
                name: control.to_owned(),
                value,
                reason: "value must be finite (not NaN or infinity)".to_owned(),
            });
        }
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

    /// Nombres de todas las variables conocidas (para descubrimiento).
    pub fn list_variables(&self) -> Vec<String> {
        self.runtime.store().list_variables()
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
        assert!(!sim.list_variables().iter().any(|n| n == "OVHD_ELEC_BAT_1_PB_IS_ATUO"));
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
    fn set_environment_through_the_api() {
        let mut sim = Sim::new();
        sim.set_environment(1000.0, 0.0, 5.0, 1013.25);
        sim.step(1000);

        let s = sim.get(&["SIM ON GROUND", "PRESSURE ALTITUDE"]).unwrap();
        assert_eq!(s["SIM ON GROUND"], 1.0);
        assert!((s["PRESSURE ALTITUDE"] - 1000.0).abs() < 1e-6);
    }
}
