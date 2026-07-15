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

use crate::environment::Environment;
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
    /// Estado del mundo exterior; se escribe entero en el store cada tick.
    environment: Environment,
    /// Tiempo de simulación acumulado, en segundos. Monótono creciente.
    sim_time: f64,
    start_state: StartState,
}

impl Runtime {
    /// Duración del tick de inicialización (ver [`Runtime::initialize`]). Un
    /// solo tick basta para que las máquinas de estado transicionen fuera de su
    /// estado de arranque; 100 ms es un delta nominal y no acumula ningún
    /// retardo (todos los controles están en OFF durante la inicialización).
    const INIT_TICK: Duration = Duration::from_millis(100);

    /// Crea un runtime en el `start_state` dado.
    pub fn new(start_state: StartState) -> Self {
        let mut store = VariableStore::new();
        // El registro se comparte con el avión: los identificadores que el avión
        // cachea en la construcción coinciden con los de nuestro índice, de modo
        // que escribir una variable por nombre acaba bajo el mismo id que el
        // avión leerá en el tick.
        let simulation = Simulation::new(start_state, A320::new, &mut store.registry);
        let environment = Environment::cold_and_dark_ground();
        let mut runtime = Self {
            simulation,
            store,
            environment,
            sim_time: 0.0,
            start_state,
        };
        // Deja el store en un estado de mundo válido ya antes del primer tick
        // (p. ej. para leer variables sin haber ticado todavía).
        runtime.environment.write_all(&mut runtime.store);
        // Tick de inicialización con los controles en su default (todo OFF) para
        // que las máquinas de estado internas del avión —que no viven en el store
        // y no son sembrables (D-007)— arranquen en su estado coherente antes de
        // que el caller pueda escribir nada. Ver D-012 y `initialize`.
        runtime.initialize();
        runtime
    }

    /// Tick de inicialización: avanza la simulación **un** tick con los controles
    /// en su default (cold & dark, todo OFF) y luego **restaura el reloj a 0**.
    ///
    /// ## Por qué hace falta (issue #39 / D-012)
    ///
    /// Algunos subsistemas guardan estado en máquinas de estado privadas del
    /// avión (no en el store, luego no sembrables — D-007) cuya transición
    /// inicial depende de leer sus controles en OFF durante el primer tick. El
    /// caso de referencia es el `BatteryChargeLimiter`
    /// (`electrical/battery_charge_limiter.rs`): arranca en `State::Open`
    /// (`:25`) y la única vía **robusta** hacia el contactor cerrado en tierra es
    /// `Open -> Off -> Closed::from_off()` (`:176`), que exige que el pulsador de
    /// batería se lea en OFF al menos un tick para pasar por `Off`. Si el caller
    /// escribe `BAT_x_PB_IS_AUTO=1` **antes** del primer tick, el BCL evalúa en
    /// `Open` con el pulsador ya en AUTO y ninguna condición de `should_close`
    /// (`:243`) puede cumplirse en cold & dark: la rama de tierra
    /// (`on_ground_at_low_speed_with_unpowered_ac_buses`, `:525`) exige
    /// `lgciu1.left_and_right_gear_compressed`, que devuelve `false` con el
    /// LGCIU sin alimentar (`landing_gear/mod.rs:518`, `self.is_powered && …`);
    /// y la rama de carga exige el bat bus por encima de 27 V (`:298`), que está
    /// muerto precisamente porque el contactor no cierra. El BCL se queda en
    /// `Open` **para siempre** y el DC BAT bus nunca se alimenta.
    ///
    /// El culpable es estado privado del avión, no una variable del store: no se
    /// puede sembrar. El único resorte disponible es avanzar la simulación una
    /// vez con los controles en default —exactamente lo que hacía el patrón
    /// "tica primero" que sí funcionaba—, dejando el BCL en `Off` y listo para
    /// cerrar cuando el caller ponga las baterías en AUTO.
    ///
    /// Restaurar el reloj a 0 preserva la semántica de "sim_time real y monótono
    /// desde 0" y no altera el cold & dark de D-007: tras el tick todo sigue en
    /// default y sin alimentar (los buses de la red siguen muertos), solo que las
    /// máquinas de estado internas ya están inicializadas.
    fn initialize(&mut self) {
        self.tick(Self::INIT_TICK);
        self.sim_time = 0.0;
    }

    /// Acceso de solo lectura al estado del mundo exterior.
    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    /// Fija el mundo exterior con los knobs de alto nivel del contrato de la
    /// API (`set_environment`). El resto de la tabla se deriva de forma
    /// coherente y se escribe en cada tick.
    pub fn set_environment(
        &mut self,
        altitude_ft: f64,
        indicated_airspeed_kt: f64,
        oat_celsius: f64,
        qnh_hpa: f64,
    ) {
        self.environment
            .set(altitude_ft, indicated_airspeed_kt, oat_celsius, qnh_hpa);
        // Aplica ya para que una lectura inmediata (sin tick) sea coherente.
        self.environment.write_all(&mut self.store);
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
    /// Orden (nota de diseño): escribir la tabla completa de mundo →
    /// `simulation.tick` → avanzar el reloj. Las escrituras de control ya viven
    /// en el store; el entorno se reescribe entero cada tick porque
    /// `UpdateContext` lo relee cada tick.
    fn tick(&mut self, delta: Duration) {
        self.environment.write_all(&mut self.store);
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
        assert!(
            (rt.sim_time() - 1.0).abs() < 1e-9,
            "sim_time = {}",
            rt.sim_time()
        );

        rt.step(1000);
        assert!(
            (rt.sim_time() - 2.0).abs() < 1e-9,
            "sim_time = {}",
            rt.sim_time()
        );
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
        assert!(
            (rt.sim_time() - 2.0).abs() < 1e-9,
            "sim_time = {}",
            rt.sim_time()
        );
    }

    #[test]
    fn defaults_only_read_as_stationary_on_ground_no_manual_fixups() {
        // El spike tenía que fijar a mano on_ground/alt/temp/ias. Con el perfil
        // por defecto del runtime, tras un tick el mundo ya es coherente en
        // tierra sin ningún setup del caller.
        let mut rt = Runtime::apron_cold_and_dark();
        rt.step(1000);

        assert_eq!(rt.read_by_name("SIM ON GROUND"), 1.0);
        assert_eq!(rt.read_by_name("AIRSPEED INDICATED"), 0.0);
        assert_eq!(rt.read_by_name("PLANE ALT ABOVE GROUND"), 0.0);
        // AMBIENT PRESSURE en inHg (no hPa/Pa).
        assert!((rt.read_by_name("AMBIENT PRESSURE") - 29.92).abs() < 1e-6);
    }

    #[test]
    fn set_environment_updates_the_world() {
        let mut rt = Runtime::apron_cold_and_dark();
        rt.set_environment(2000.0, 0.0, -10.0, 1013.25);
        rt.step(1000);

        // uom convierte ft<->m, así que hay redondeo de coma flotante.
        assert!((rt.read_by_name("PRESSURE ALTITUDE") - 2000.0).abs() < 1e-6);
        assert!((rt.read_by_name("AMBIENT TEMPERATURE") - (-10.0)).abs() < 1e-6);
        assert_eq!(rt.read_by_name("SIM ON GROUND"), 1.0); // sigue en tierra
        assert_eq!(rt.read_by_name("PLANE ALT ABOVE GROUND"), 0.0); // coherente
    }

    /// Regresión del issue #39: escribir los pulsadores de batería **antes** de
    /// cualquier tick del caller debe comportarse igual que escribirlos después
    /// de un tick. Sin el tick de inicialización de `Runtime::new`, el
    /// `BatteryChargeLimiter` evaluaba su primer tick en `Open` con el pulsador
    /// ya en AUTO y quedaba abierto para siempre (ver D-012).
    #[test]
    fn writes_before_the_first_tick_do_not_wedge_the_battery_contactor() {
        let mut rt = Runtime::apron_cold_and_dark();

        // Caso B del issue: set como PRIMERA operación, sin ningún tick previo.
        rt.write_by_name(BAT_1_PB_IS_AUTO, 1.0);
        rt.write_by_name(BAT_2_PB_IS_AUTO, 1.0);
        rt.run(2.0, 5.0); // mismo settling que el caso A

        // Los pulsadores no se machacan...
        assert_eq!(rt.read_by_name(BAT_1_PB_IS_AUTO), 1.0);
        assert_eq!(rt.read_by_name(BAT_2_PB_IS_AUTO), 1.0);
        // ...y el DC BAT bus cobra vida, como en el caso "tick primero".
        assert!(
            is_true(rt.read_by_name(DC_BAT_BUS_POWERED)),
            "DC BAT debería alimentarse aunque el set llegue antes del primer tick"
        );
        // El tick de inicialización no adelanta el reloj del caller.
        assert!(
            (rt.sim_time() - 2.0).abs() < 1e-9,
            "sim_time = {}",
            rt.sim_time()
        );
    }

    #[test]
    fn apron_reproduces_spike_cold_and_dark_then_battery_on() {
        let mut rt = Runtime::apron_cold_and_dark();

        // Cold & dark: sin seeding, los pulsadores de batería leen 0 (no AUTO).
        // Toda la red debe estar sin alimentar.
        rt.step(1000);
        assert!(
            !is_true(rt.read_by_name(DC_BAT_BUS_POWERED)),
            "DC BAT off en cold & dark"
        );
        assert!(
            !is_true(rt.read_by_name(AC_1_BUS_POWERED)),
            "AC 1 off en cold & dark"
        );

        // Baterías ON: el DC BAT bus debe cobrar vida (solo baterías, sin AC).
        rt.write_by_name(BAT_1_PB_IS_AUTO, 1.0);
        rt.write_by_name(BAT_2_PB_IS_AUTO, 1.0);
        rt.run(2.0, 5.0); // settling: 10 ticks de 200 ms, como el spike

        assert!(
            is_true(rt.read_by_name(DC_BAT_BUS_POWERED)),
            "DC BAT ON con baterías"
        );
        assert!(
            !is_true(rt.read_by_name(AC_1_BUS_POWERED)),
            "AC 1 sigue off (sin fuente AC)"
        );
    }
}
