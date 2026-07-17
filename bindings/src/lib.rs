//! PyO3 bindings: la seam entre el core Rust y los frontends Python.
//!
//! Expone `a320_sim_core::api::Sim` como una clase Python síncrona `Sim`. Por el
//! borde FFI solo cruzan tipos triviales — `f64`/`bool`/`str`/list/dict — nunca
//! tipos de FBW (`VariableIdentifier`, `UpdateContext` y compañía se quedan en
//! Rust). Los `ApiError` tipados afloran como excepciones Python con mensaje
//! útil; ningún panic cruza el FFI.
//!
//! Esta capa no tiene lógica de simulación: es un envoltorio 1:1 del contrato de
//! `api::Sim`. `list_controls()` (issue #10/#12) y la inyección de fallos (#14)
//! ya están expuestos; `read_ecam()` (#15) se añadirá cuando exista en el core;
//! no se stubbea.

use std::collections::BTreeMap;

use a320_sim_core::api::{ApiError, Sim as CoreSim};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

// Jerarquía de excepciones: un LLM o un REPL pueden atrapar `SimError` para todo,
// o discriminar por subtipo. El mensaje viene del `Display` de `ApiError`, que ya
// es accionable (dice qué falló y cómo descubrir nombres válidos).
create_exception!(
    a320_sim,
    SimError,
    PyException,
    "Base de todas las excepciones de a320_sim."
);
create_exception!(
    a320_sim,
    UnknownControlError,
    SimError,
    "Nombre de control/variable ausente del catálogo del avión (típicamente un typo)."
);
create_exception!(
    a320_sim,
    BadValueError,
    SimError,
    "Valor no admisible para un control (p. ej. NaN o infinito)."
);
create_exception!(
    a320_sim,
    UnknownFailureError,
    SimError,
    "Id de fallo ausente del catálogo (usa list_failures() para descubrir los válidos)."
);

/// Traduce un `ApiError` del core a la excepción Python correspondiente,
/// preservando el mensaje del `Display`.
///
/// El `match` es **exhaustivo a propósito** (sin brazo `_ =>`): una variante
/// nueva en `ApiError` debe romper la compilación justo aquí, que es donde hay
/// que decidir qué excepción ve Python. Un catch-all convertiría ese momento en
/// un `SimError` genérico silencioso.
fn to_pyerr(err: ApiError) -> PyErr {
    let msg = err.to_string();
    match err {
        ApiError::UnknownControl { .. } => UnknownControlError::new_err(msg),
        ApiError::BadValue { .. } => BadValueError::new_err(msg),
        ApiError::UnknownFailure { .. } => UnknownFailureError::new_err(msg),
    }
}

/// Simulador headless de los sistemas del A320. Arranca cold & dark en el apron.
///
/// Envuelve el runtime persistente del core. Todos los métodos son síncronos:
/// `Simulation<A320>` no es async, así que esto es un objeto Python normal.
///
/// `unsendable`: el avión de FBW usa `Rc`/`RefCell` internamente (no es `Send`),
/// así que la instancia queda ligada al hilo Python que la creó. Si otro hilo
/// la toca, PyO3 lanza un `RuntimeError` en Python — un error explícito, no un
/// panic a través del FFI. Para la CLI y el servidor MCP (acceso secuencial
/// desde un hilo) es el contrato correcto.
#[pyclass(name = "Sim", unsendable)]
struct PySim {
    inner: CoreSim,
}

#[pymethods]
impl PySim {
    /// Crea un avión nuevo en cold & dark (apron).
    #[new]
    fn new() -> Self {
        PySim {
            inner: CoreSim::new(),
        }
    }

    /// Actúa un control escribiendo su variable de entrada.
    ///
    /// `value` acepta int/float/bool de Python (True/False -> 1.0/0.0).
    /// Lanza `UnknownControlError` si el nombre no está en el catálogo y
    /// `BadValueError` si el valor no es finito.
    fn set(&mut self, control: &str, value: f64) -> PyResult<()> {
        self.inner.set(control, value).map_err(to_pyerr)
    }

    /// Lee las variables pedidas y devuelve un dict nombre->valor.
    ///
    /// Lanza `UnknownControlError` en la primera variable desconocida.
    fn get(&self, vars: Vec<String>) -> PyResult<BTreeMap<String, f64>> {
        let refs: Vec<&str> = vars.iter().map(String::as_str).collect();
        self.inner.get(&refs).map_err(to_pyerr)
    }

    /// Avanza la simulación `dt_ms` milisegundos en un único tick.
    fn step(&mut self, dt_ms: u64) {
        self.inner.step(dt_ms);
    }

    /// Avanza `seconds` segundos ejecutando ticks a `rate` Hz.
    #[pyo3(signature = (seconds, rate))]
    fn run(&mut self, seconds: f64, rate: f64) {
        self.inner.run(seconds, rate);
    }

    /// Fija el mundo exterior con knobs de alto nivel.
    #[pyo3(signature = (altitude_ft, indicated_airspeed_kt, oat_celsius, qnh_hpa))]
    fn set_environment(
        &mut self,
        altitude_ft: f64,
        indicated_airspeed_kt: f64,
        oat_celsius: f64,
        qnh_hpa: f64,
    ) {
        self.inner
            .set_environment(altitude_ft, indicated_airspeed_kt, oat_celsius, qnh_hpa);
    }

    /// Volcado completo del estado: dict con todas las variables conocidas.
    fn snapshot(&self) -> BTreeMap<String, f64> {
        self.inner.snapshot()
    }

    /// Nombres de todas las variables conocidas (para descubrimiento).
    fn list_variables(&self) -> Vec<String> {
        self.inner.list_variables()
    }

    /// Catálogo curado de controles accionables (issue #10/#12).
    ///
    /// Devuelve una lista de dicts, uno por control, con solo tipos triviales
    /// (todos `str`) para cruzar el FFI: `name` (nombre amigable), `lvar` (LVAR
    /// subyacente), `kind` (`bool`/`enum`/`float`), `valid_values` (descripción
    /// legible del rango/conjunto admisible), `description`, `group` (sistema) y
    /// `domain` (`cockpit`/`world`). Lo consume la CLI para autocompletar y el
    /// MCP para el esquema de `set_control`.
    fn list_controls(&self) -> Vec<BTreeMap<String, String>> {
        self.inner
            .list_controls()
            .into_iter()
            .map(|c| {
                let mut d = BTreeMap::new();
                d.insert("name".to_owned(), c.name.to_owned());
                d.insert("lvar".to_owned(), c.lvar.to_owned());
                d.insert("kind".to_owned(), c.kind.as_str().to_owned());
                d.insert("valid_values".to_owned(), c.valid.describe());
                d.insert("description".to_owned(), c.description.to_owned());
                d.insert("group".to_owned(), c.group.as_str().to_owned());
                d.insert("domain".to_owned(), c.domain.as_str().to_owned());
                d
            })
            .collect()
    }

    /// Catálogo curado de fallos inyectables (issue #14).
    ///
    /// Lista de dicts, uno por fallo, todo `str` para cruzar el FFI: `id` (el
    /// identificador estable nuestro, p. ej. `elec.tr.1`), `ata` (el id numérico
    /// que usa FBW para el mismo fallo, para cruzar con upstream),
    /// `description` y `group` (sistema).
    fn list_failures(&self) -> Vec<BTreeMap<String, String>> {
        self.inner
            .list_failures()
            .into_iter()
            .map(|f| {
                let mut d = BTreeMap::new();
                d.insert("id".to_owned(), f.id.to_owned());
                d.insert("ata".to_owned(), f.ata.to_string());
                d.insert("description".to_owned(), f.description.to_owned());
                d.insert("group".to_owned(), f.group.as_str().to_owned());
                d
            })
            .collect()
    }

    /// Activa un fallo por su id del catálogo. Surte efecto en el siguiente tick.
    ///
    /// Lanza `UnknownFailureError` si el id no está catalogado.
    fn inject_failure(&mut self, id: &str) -> PyResult<()> {
        self.inner.inject_failure(id).map_err(to_pyerr)
    }

    /// Desactiva un fallo por su id. Idempotente (limpiar uno inactivo no falla).
    ///
    /// Lanza `UnknownFailureError` si el id no está catalogado.
    fn clear_failure(&mut self, id: &str) -> PyResult<()> {
        self.inner.clear_failure(id).map_err(to_pyerr)
    }

    /// Ids de los fallos activos ahora mismo, ordenados.
    fn active_failures(&self) -> Vec<String> {
        self.inner
            .active_failures()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    /// Tiempo de simulación acumulado en segundos (monótono).
    fn sim_time(&self) -> f64 {
        self.inner.sim_time()
    }

    fn __repr__(&self) -> String {
        format!("<a320_sim.Sim t={:.3}s>", self.inner.sim_time())
    }
}

/// Módulo de extensión `a320_sim`.
#[pymodule]
fn a320_sim(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySim>()?;
    m.add("SimError", m.py().get_type::<SimError>())?;
    m.add(
        "UnknownControlError",
        m.py().get_type::<UnknownControlError>(),
    )?;
    m.add("BadValueError", m.py().get_type::<BadValueError>())?;
    m.add(
        "UnknownFailureError",
        m.py().get_type::<UnknownFailureError>(),
    )?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
