//! Runtime persistente: envuelve `Simulation<A320>` directamente.
//!
//! El test bed de FBW (`SimulationTestBed`) sirve para tests cortos pero es un
//! callejón sin salida para un runtime vivo: `run_with_delta` **hardcodea
//! `simulation_time = 100.`** (`test.rs:296`). El FWC y muchos sistemas razonan
//! sobre tiempo transcurrido, así que un reloj clavado en 100 s no es cosmético.
//!
//! Aquí manejamos `Simulation<A320>` (`mod.rs:359`), que es usable standalone:
//!
//! - `Simulation::new(start_state, A320::new, &mut registry)` — instancia.
//! - `tick(delta, simulation_time, &mut reader_writer)` — un tick con `delta` y
//!   tiempo controlados por nosotros.
//!
//! El bucle por tick (nota de diseño `docs/fase1-runtime.md`):
//!
//! ```text
//! aplicar escrituras pendientes (controles / entorno)  <- ya viven en el store
//! simulation.tick(delta, sim_time, &mut reader_writer)
//! sim_time += delta
//! servir lecturas (get / snapshot)
//! ```
//!
//! ## Nota sobre el seeding
//!
//! El test bed, tras construir, hace `seed()`: recorre el avión y escribe su
//! estado inicial programado (p. ej. pulsadores que arrancan en ON). Ese paso
//! usa `Simulation::accept`, que es **privado** en FBW, así que no es accesible
//! desde fuera del crate. Consecuencia: nuestro arranque `Apron` **no** siembra
//! los estados iniciales — todos los pulsadores leen su default (0/OFF). Para el
//! slice eléctrico esto ES el cold & dark puro (de hecho el spike tenía que
//! forzar los pulsadores de batería a `false` precisamente para deshacer lo que
//! el seeding los había puesto en AUTO). Ver `docs/decisiones.md` (D-007).

use std::time::Duration;

use a320_systems::A320;
use systems::simulation::{Simulation, StartState};

use crate::variables::VariableStore;

/// Runtime persistente del A320 headless.
///
/// Instancia el avión una sola vez y lo tica repetidamente; el estado (y el
/// almacén de variables) persiste entre ticks. El tiempo de simulación es real
/// y monótono (empieza en 0 y crece con cada `delta`), a diferencia del 100 s
/// clavado del test bed.
pub struct Runtime {
    simulation: Simulation<A320>,
    store: VariableStore,
    /// Tiempo de simulación acumulado, en segundos. Monótono creciente.
    sim_time: f64,
    start_state: StartState,
}

impl Runtime {
    /// Crea un runtime en el `start_state` dado.
    pub fn new(start_state: StartState) -> Self {
        let mut store = VariableStore::new();
        // El registro se comparte con el avión: los identificadores que el avión
        // cachea en la construcción coinciden con los de nuestro índice, de modo
        // que escribir una variable por nombre acaba bajo el mismo id que el
        // avión leerá en el tick.
        let simulation = Simulation::new(start_state, A320::new, &mut store.registry);
        let mut runtime = Self {
            simulation,
            store,
            sim_time: 0.0,
            start_state,
        };
        runtime.apply_default_ground_environment();
        runtime
    }

    /// Entorno mínimo de tierra para que el avión pueda ticar sin producir NaN.
    ///
    /// Con el store a cero, `AMBIENT PRESSURE = 0` hace que ratios de presión en
    /// los sistemas produzcan NaN y un `clamp` interno haga panic. Escribimos el
    /// mismo conjunto mínimo que el spike de Fase 0 fijaba a mano (presión y
    /// temperatura estándar de campo, en tierra, IAS 0). Estos valores no los
    /// escribe nunca el avión (son entradas de "mundo") y persisten en el store,
    /// así que basta con fijarlos una vez.
    ///
    /// Nota de unidades: `AMBIENT PRESSURE` va en **inHg** y `AMBIENT
    /// TEMPERATURE` en **°C** — se escriben en esas unidades directamente. El
    /// perfil completo de entorno (todos los simvars, escritos cada tick) llega
    /// en el issue #8; esto es el mínimo imprescindible para el tick loop.
    fn apply_default_ground_environment(&mut self) {
        self.store.write_by_name("IS_READY", 1.0);
        self.store.write_by_name("SIM ON GROUND", 1.0);
        self.store.write_by_name("AMBIENT PRESSURE", 29.92); // inHg (ISA nivel del mar)
        self.store.write_by_name("AMBIENT TEMPERATURE", 15.0); // °C
        self.store.write_by_name("AMBIENT DENSITY", 1.225); // kg/m^3
        self.store.write_by_name("PRESSURE ALTITUDE", 0.0); // ft
        self.store.write_by_name("PLANE ALT ABOVE GROUND", 0.0); // ft
        self.store.write_by_name("AIRSPEED INDICATED", 0.0); // kt
    }

    /// Arranque cold & dark en el apron (el punto de partida del spike de Fase 0).
    pub fn apron_cold_and_dark() -> Self {
        Self::new(StartState::Apron)
    }

    /// Tiempo de simulación acumulado en segundos (monótono).
    pub fn sim_time(&self) -> f64 {
        self.sim_time
    }

    /// Estado de arranque con el que se instanció el avión.
    pub fn start_state(&self) -> StartState {
        self.start_state
    }

    /// Acceso de solo lectura al almacén de variables.
    pub fn store(&self) -> &VariableStore {
        &self.store
    }

    /// Acceso mutable al almacén de variables (para escribir controles/entorno).
    pub fn store_mut(&mut self) -> &mut VariableStore {
        &mut self.store
    }

    /// Escribe una variable de entrada por nombre (control, entorno...).
    pub fn write_by_name(&mut self, name: &str, value: f64) {
        self.store.write_by_name(name, value);
    }

    /// Lee una variable por nombre (acuña id si es nueva).
    pub fn read_by_name(&mut self, name: &str) -> f64 {
        self.store.read_by_name(name)
    }

    /// Ejecuta un único tick de duración `delta`.
    ///
    /// Las escrituras pendientes ya viven en el store (se aplicaron con
    /// `write_by_name` antes de llamar). Se pasa el tiempo de simulación al
    /// inicio del tick y luego se avanza el reloj en `delta`.
    fn tick(&mut self, delta: Duration) {
        self.simulation
            .tick(delta, self.sim_time, &mut self.store.reader_writer);
        self.sim_time += delta.as_secs_f64();
    }

    /// Avanza la simulación `dt_ms` milisegundos en un solo tick.
    pub fn step(&mut self, dt_ms: u64) {
        self.tick(Duration::from_millis(dt_ms));
    }

    /// Avanza `seconds` segundos ejecutando ticks a `rate` Hz.
    ///
    /// El número de ticks es `round(seconds * rate)`, cada uno de `1/rate`
    /// segundos. P. ej. `run(2.0, 5.0)` = 10 ticks de 200 ms (el patrón de
    /// settling del spike). Un `rate` <= 0 o `seconds` <= 0 no hace nada.
    pub fn run(&mut self, seconds: f64, rate: f64) {
        if seconds <= 0.0 || rate <= 0.0 {
            return;
        }
        let dt = Duration::from_secs_f64(1.0 / rate);
        let ticks = (seconds * rate).round() as u64;
        for _ in 0..ticks {
            self.tick(dt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nombres de LVARs eléctricos y pulsadores (del spike de Fase 0).
    const BAT_1_PB_IS_AUTO: &str = "OVHD_ELEC_BAT_1_PB_IS_AUTO";
    const BAT_2_PB_IS_AUTO: &str = "OVHD_ELEC_BAT_2_PB_IS_AUTO";
    const DC_BAT_BUS_POWERED: &str = "ELEC_DC_BAT_BUS_IS_POWERED";
    const AC_1_BUS_POWERED: &str = "ELEC_AC_1_BUS_IS_POWERED";

    fn is_true(v: f64) -> bool {
        v != 0.0
    }

    #[test]
    fn sim_time_is_real_and_monotonic() {
        let mut rt = Runtime::apron_cold_and_dark();
        assert_eq!(rt.sim_time(), 0.0);

        rt.step(1000);
        // No es el 100 hardcodeado del test bed.
        assert!((rt.sim_time() - 1.0).abs() < 1e-9, "sim_time = {}", rt.sim_time());

        rt.step(1000);
        assert!((rt.sim_time() - 2.0).abs() < 1e-9, "sim_time = {}", rt.sim_time());
    }

    #[test]
    fn n_steps_of_dt_accumulate_to_n_times_dt_and_control_persists() {
        let mut rt = Runtime::apron_cold_and_dark();

        // Una escritura de control debe sobrevivir a los ticks.
        rt.write_by_name(BAT_1_PB_IS_AUTO, 1.0);

        let n = 10u64;
        let dt_ms = 200u64;
        for _ in 0..n {
            rt.step(dt_ms);
        }

        let expected = (n * dt_ms) as f64 / 1000.0;
        assert!(
            (rt.sim_time() - expected).abs() < 1e-9,
            "sim_time {} != {}",
            rt.sim_time(),
            expected
        );
        assert_eq!(rt.read_by_name(BAT_1_PB_IS_AUTO), 1.0, "control persisted");
    }

    #[test]
    fn run_subdivides_into_ticks() {
        let mut rt = Runtime::apron_cold_and_dark();
        rt.run(2.0, 5.0); // 10 ticks de 200 ms
        assert!((rt.sim_time() - 2.0).abs() < 1e-9, "sim_time = {}", rt.sim_time());
    }

    #[test]
    fn apron_reproduces_spike_cold_and_dark_then_battery_on() {
        let mut rt = Runtime::apron_cold_and_dark();

        // Cold & dark: sin seeding, los pulsadores de batería leen 0 (no AUTO).
        // Toda la red debe estar sin alimentar.
        rt.step(1000);
        assert!(!is_true(rt.read_by_name(DC_BAT_BUS_POWERED)), "DC BAT off en cold & dark");
        assert!(!is_true(rt.read_by_name(AC_1_BUS_POWERED)), "AC 1 off en cold & dark");

        // Baterías ON: el DC BAT bus debe cobrar vida (solo baterías, sin AC).
        rt.write_by_name(BAT_1_PB_IS_AUTO, 1.0);
        rt.write_by_name(BAT_2_PB_IS_AUTO, 1.0);
        rt.run(2.0, 5.0); // settling: 10 ticks de 200 ms, como el spike

        assert!(is_true(rt.read_by_name(DC_BAT_BUS_POWERED)), "DC BAT ON con baterías");
        assert!(!is_true(rt.read_by_name(AC_1_BUS_POWERED)), "AC 1 sigue off (sin fuente AC)");
    }
}
