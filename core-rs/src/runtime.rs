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
//!
//! ## Nota sobre los fallos (Fase 2, #14)
//!
//! Los fallos **no viven en el store**: el vendor los propaga por un canal
//! aparte (`Simulation::update_active_failures` → `receive_failure` en cada
//! `SimulationElement`), no por el ciclo read/write de variables. Y el contrato
//! es **declarativo**: cada llamada reemplaza el conjunto activo entero, no es
//! un toggle. Por eso el runtime es el dueño del `FxHashSet<FailureType>` y lo
//! reenvía en cada tick — el mismo patrón que usa el test bed de FBW
//! (`test.rs:329-339`), pero sin el test bed.

use std::time::Duration;

use a320_systems::A320;
use rustc_hash::FxHashSet;
use systems::failures::FailureType;
use systems::simulation::{Simulation, StartState};

use crate::engine::{EngineModel, MODE_SELECTOR_LVAR};
use crate::environment::Environment;
use crate::variables::VariableStore;

/// Carga de combustible por defecto, en **galones US** por tanque.
///
/// El Rust de FBW no modela consumo ni crossfeed: los simvars
/// `FUEL TANK * QUANTITY` son *entradas* de mundo en galones que `FuelTank::read`
/// convierte a kg multiplicando por `FUEL_GALLONS_TO_KG = 3.039075693483925`
/// (`fbw-common/.../systems/src/fuel/mod.rs:12` y `:97-100`). Los nombres y
/// capacidades salen de `A320_FUEL`
/// (`a320_systems/src/fuel/mod.rs:53-79`): center 2179 gal, mains 1816 gal,
/// aux 228 gal.
///
/// **Criterio del reparto** (~6 400 kg, una carga de bloque realista de corto
/// radio): el repostaje real del A320 llena las alas antes que el central —
/// células aux (outer) llenas, el resto a partes iguales en los mains, center
/// vacío. 2 × 228 + 2 × 825 = 2 106 gal ≈ 6 400 kg. El APU bebe del tanque
/// **left main** (`a320_systems/src/lib.rs:171` →
/// `left_inner_tank_has_fuel_remaining()`, `fuel/mod.rs:134-137`), que queda
/// bien servido con 825 gal.
///
/// Es un **seed, no entorno**: se escribe UNA vez en [`Runtime::new`] (antes
/// del tick de inicialización) y ningún tick lo reescribe — un escenario puede
/// vaciar un tanque con `set` y el runtime no se lo machaca.
const FUEL_SEED_GALLONS: &[(&str, f64)] = &[
    ("FUEL TANK CENTER QUANTITY", 0.0),
    ("FUEL TANK LEFT MAIN QUANTITY", 825.0),
    ("FUEL TANK LEFT AUX QUANTITY", 228.0),
    ("FUEL TANK RIGHT MAIN QUANTITY", 825.0),
    ("FUEL TANK RIGHT AUX QUANTITY", 228.0),
];

/// Seed de los controles de motor, escrito UNA vez en [`Runtime::new`] (mismo
/// patrón que [`FUEL_SEED_GALLONS`]: los ticks no lo reasientan).
///
/// - `ENG_MASTER_{1,2}` son LVARs **nuestros** (D-020): en MSFS el engine
///   master vive en el fuel system C++ y ningún elemento del Rust del vendor
///   lo registra. Sembrarlos a 0 (OFF) los deja registrados desde el arranque
///   (los exige `every_catalog_lvar_is_registered_after_a_tick`) y en el estado
///   cold & dark correcto.
/// - El selector de modo (`TURB ENG IGNITION SWITCH EX1:1`) descansa en
///   **NORM = 1** en el panel real, pero nadie lo escribe en el Rust del
///   vendor y el default de una var no escrita es 0.0 = **CRANK**
///   (`EngineModeSelector`, `fbw-common/.../pneumatic/mod.rs:764-782`). Sin
///   este seed, el FADEC de pneumatic leería CRANK para siempre.
const ENGINE_CONTROL_SEED: &[(&str, f64)] = &[
    ("ENG_MASTER_1", 0.0),
    ("ENG_MASTER_2", 0.0),
    (MODE_SELECTOR_LVAR, 1.0),
];

/// Seed de **estados de reposo del panel** que el `seed()` del vendor
/// materializaría y D-007 nos impide ejecutar. Escrito UNA vez en
/// [`Runtime::new`] (mismo patrón que el selector de modo en
/// [`ENGINE_CONTROL_SEED`]; ver D-021). Sin estos seeds, el LVAR no escrito lee
/// 0.0, que en ambos casos significa algo distinto del reposo del panel real:
///
/// - **GEN 1 LINE** (panel EMER ELEC): el vendor lo construye
///   `OnOffFaultPushButton::new_on(context, "EMER_ELEC_GEN_1_LINE")`
///   (`a320_systems/src/electrical/mod.rs:391`) — reposo **ON** (solo se apaga
///   en el procedimiento SMOKE). Sin seed, `gen_1_provides_power` — que exige
///   `generator_1_line_is_on()` además del pulsador GEN 1
///   (`electrical/alternating_current.rs:432-435`) — sería falso para siempre:
///   el GEN 1 giraría a 115 V sin cerrar su contactor, con `ENG 1 GEN FAULT`
///   en un avión sano. El GEN 2 no tiene condición equivalente (`:436-438`).
/// - **Selector X BLEED** (panel neumático): el vendor lo construye
///   `CrossBleedValveSelectorKnob::new_auto` con el LVAR
///   `KNOB_OVHD_AIRCOND_XBLEED_Position`
///   (`fbw-common/.../pneumatic/mod.rs:462-470`; enum SHUT=0/AUTO=1/OPEN=2,
///   `:487-491`) — reposo **AUTO**. Sin seed leería 0 = **SHUT** y la válvula
///   de crossbleed jamás abriría (en AUTO abre cuando abre la válvula de APU
///   bleed, `a320_systems/src/pneumatic.rs:986-1008`): el motor 2 no podría
///   arrancar nunca con aire del APU.
///
/// Ambos siguen siendo accionables como controles del catálogo (`gen_1_line`,
/// `xbleed`): el seed fija el reposo, no congela el valor.
const PANEL_RESTING_SEED: &[(&str, f64)] = &[
    ("OVHD_EMER_ELEC_GEN_1_LINE_PB_IS_ON", 1.0),
    ("KNOB_OVHD_AIRCOND_XBLEED_Position", 1.0),
];

/// Seed de **mundo**: no hay tug de pushback enganchado.
///
/// `PushbackTug` lee `PUSHBACK STATE` y trata **3 = sin pushback**; cualquier
/// otro valor (incluido el 0.0 de una var no escrita: "pushback recto") cuenta
/// como pushback en curso e inserta el bypass pin de la dirección del morro
/// (`fbw-common/.../hydraulic/pushback.rs:24-31,60-69`). Con el pin insertado,
/// el PTU en AUTO queda inhibido en tierra con un solo engine master ON — la
/// rama `!parking_brake && !bypass_pin` de
/// `A320PowerTransferUnitController::update`
/// (`a320_systems/src/hydraulic/mod.rs:3491-3497`) nunca habilita. Es un seed y
/// no entorno por el mismo motivo que el fuel (D-018): un escenario futuro de
/// pushback debe poder escribirlo sin que el tick se lo machaque.
const WORLD_STATE_SEED: &[(&str, f64)] = &[("PUSHBACK STATE", 3.0)];

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
    /// Modelos de motor propios (D-019): generan por tick los simvars de motor
    /// que el vendor lee como entrada pura (N1/N2/estado/empuje/starter).
    engines: [EngineModel; 2],
    /// Tiempo de simulación acumulado, en segundos. Monótono creciente.
    sim_time: f64,
    start_state: StartState,
    /// Conjunto de fallos activos; se reenvía entero al avión cada tick.
    active_failures: FxHashSet<FailureType>,
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
            engines: [EngineModel::new(1), EngineModel::new(2)],
            sim_time: 0.0,
            start_state,
            active_failures: FxHashSet::default(),
        };
        // Deja el store en un estado de mundo válido ya antes del primer tick
        // (p. ej. para leer variables sin haber ticado todavía).
        runtime.environment.write_all(&mut runtime.store);
        // Siembra el combustible ANTES del tick de inicialización, para que el
        // primer estado observable (y la primera `read_ecam`) ya sea el de un
        // avión con su carga por defecto. Una sola escritura, ver el doc de
        // `FUEL_SEED_GALLONS`.
        for &(tank, gallons) in FUEL_SEED_GALLONS {
            runtime.store.write_by_name(tank, gallons);
        }
        // Siembra los controles de motor (masters OFF, selector en NORM) por
        // los motivos del doc de `ENGINE_CONTROL_SEED`. Una sola escritura: son
        // controles de cabina y los ticks no deben machacar lo que el caller
        // escriba después.
        for &(name, value) in ENGINE_CONTROL_SEED {
            runtime.store.write_by_name(name, value);
        }
        // Siembra los reposos de panel (GEN 1 LINE a ON, X BLEED a AUTO) y el
        // mundo sin tug de pushback — ver los docs de `PANEL_RESTING_SEED` /
        // `WORLD_STATE_SEED` y D-021. Una sola escritura, como el resto.
        for &(name, value) in PANEL_RESTING_SEED.iter().chain(WORLD_STATE_SEED) {
            runtime.store.write_by_name(name, value);
        }
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
    /// Orden (nota de diseño): escribir la tabla completa de mundo → actualizar
    /// los modelos de motor → reenviar fallos → `simulation.tick` → avanzar el
    /// reloj. Las escrituras de control ya viven en el store; el entorno se
    /// reescribe entero cada tick porque `UpdateContext` lo relee cada tick.
    /// Los motores van **antes** de `simulation.tick` para que el avión lea en
    /// este mismo tick el N1/N2/estado que acaban de generar.
    fn tick(&mut self, delta: Duration) {
        self.environment.write_all(&mut self.store);
        for engine in &mut self.engines {
            engine.update(delta, &mut self.store);
        }
        // Los fallos van por su propio canal (no por el store) y el contrato es
        // declarativo: se reenvía el conjunto entero. Hacerlo aquí —y no solo al
        // mutar el set— vuelve irrelevante el orden inyectar-antes-de-ticar, que
        // es justo la clase de trampa que costó el issue #39 con las baterías.
        self.simulation
            .update_active_failures(self.active_failures.clone());
        self.simulation
            .tick(delta, self.sim_time, &mut self.store.reader_writer);
        self.sim_time += delta.as_secs_f64();
    }

    /// Activa un fallo. Idempotente: inyectar dos veces el mismo es un no-op.
    ///
    /// Surte efecto en el siguiente tick (el avión solo ve el conjunto cuando la
    /// simulación avanza).
    pub fn inject_failure(&mut self, failure_type: FailureType) {
        self.active_failures.insert(failure_type);
    }

    /// Desactiva un fallo. Idempotente: limpiar uno no activo es un no-op.
    pub fn clear_failure(&mut self, failure_type: FailureType) {
        self.active_failures.remove(&failure_type);
    }

    /// Desactiva todos los fallos activos.
    pub fn clear_all_failures(&mut self) {
        self.active_failures.clear();
    }

    /// Conjunto de fallos activos ahora mismo.
    pub fn active_failures(&self) -> &FxHashSet<FailureType> {
        &self.active_failures
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

    /// El combustible es un seed, no entorno: `Runtime::new` lo escribe una vez
    /// y el tick NO lo reasienta — vaciar un tanque debe sobrevivir a los ticks
    /// (si el entorno lo reescribiese cada tick, ningún escenario de fuel sería
    /// montable).
    #[test]
    fn fuel_seed_is_written_once_and_ticks_do_not_reassert_it() {
        let mut rt = Runtime::apron_cold_and_dark();

        // El seed es legible ya, sin ningún tick del caller.
        assert_eq!(rt.read_by_name("FUEL TANK LEFT MAIN QUANTITY"), 825.0);
        assert_eq!(rt.read_by_name("FUEL TANK LEFT AUX QUANTITY"), 228.0);
        assert_eq!(rt.read_by_name("FUEL TANK CENTER QUANTITY"), 0.0);

        // Vaciar un tanque persiste entre ticks: nadie lo machaca.
        rt.write_by_name("FUEL TANK LEFT MAIN QUANTITY", 0.0);
        rt.run(2.0, 5.0);
        assert_eq!(
            rt.read_by_name("FUEL TANK LEFT MAIN QUANTITY"),
            0.0,
            "el tick no debe reasentar el seed de combustible"
        );
    }

    /// D-021: los reposos de panel (GEN 1 LINE ON, X BLEED AUTO) y el mundo sin
    /// tug de pushback se siembran una vez — son los estados que el `seed()`
    /// del vendor materializaría y D-007 nos impide ejecutar. Siguen siendo
    /// escribibles: cambiarlos persiste entre ticks.
    #[test]
    fn panel_and_world_resting_seeds_read_their_resting_values() {
        let mut rt = Runtime::apron_cold_and_dark();
        assert_eq!(rt.read_by_name("OVHD_EMER_ELEC_GEN_1_LINE_PB_IS_ON"), 1.0);
        assert_eq!(rt.read_by_name("KNOB_OVHD_AIRCOND_XBLEED_Position"), 1.0);
        assert_eq!(rt.read_by_name("PUSHBACK STATE"), 3.0);

        rt.write_by_name("OVHD_EMER_ELEC_GEN_1_LINE_PB_IS_ON", 0.0);
        rt.run(2.0, 5.0);
        assert_eq!(
            rt.read_by_name("OVHD_EMER_ELEC_GEN_1_LINE_PB_IS_ON"),
            0.0,
            "el tick no debe reasentar el seed del GEN 1 LINE"
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
