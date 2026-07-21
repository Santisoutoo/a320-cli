//! Modelo de motor propio (Fase 4, slice 4, issue #58; ver D-019/D-020).
//!
//! El Rust de FBW **no modela el motor**: el spool termodinámico vive en el
//! FADEC C++/WASM y en MSFS. Los sistemas Rust solo **leen** simvars de motor
//! como entrada pura:
//!
//! - `LeapEngine` (`fbw-common/.../systems/src/engine/leap_engine.rs:42-45,
//!   72-78`) lee por motor `TURB ENG CORRECTED N1:{n}`, `TURB ENG CORRECTED
//!   N2:{n}`, `ENGINE_N2:{n}` (uncorrected; de ahí derivan
//!   `hydraulic_pump_output_speed`, `oil_pressure` y `is_above_minimum_idle`
//!   con umbral 55 %, `:36,100-107`) y `TURB ENG JET THRUST:{n}`.
//! - El FADEC de pneumatic (`a320_systems/src/pneumatic.rs:1587-1650`) lee
//!   `ENGINE_STATE:{n}`, `ENGINE_N2:{n}` y el selector de modo
//!   `TURB ENG IGNITION SWITCH EX1:1`.
//! - El controlador del PTU (`a320_systems/src/hydraulic/mod.rs:3449-3452,
//!   3550-3554`) lee `GENERAL ENG STARTER ACTIVE:{n}` como *engine master
//!   on/off* (sus campos se llaman `eng_{n}_master_on`).
//!
//! **Nadie transiciona `ENGINE_STATE` en el Rust del vendor** (solo su test bed
//! lo escribe): headless, ese trabajo es nuestro. Este módulo es la otra mitad
//! del borde de "mundo" que `Environment` no cubre — una máquina de estados por
//! motor con un **spool de N2 de primer orden**, determinista en función del
//! `dt` del tick (requisito del benchmark: nada de reloj de pared ni azar).
//!
//! ## Unidades con que el framework lee cada simvar generada
//!
//! `read_write_uom` (`fbw-common/.../systems/src/simulation/mod.rs:770-785`):
//! `Ratio` ↔ **percent** (N1/N2), `Mass` ↔ **pound** (el thrust se lee como
//! `Mass` en libras, `leap_engine.rs:26,76`). `ENGINE_STATE` y el selector se
//! leen como enums sobre f64 (`pneumatic/mod.rs:507-528` y `:764-782`).
//!
//! ## Simplificaciones (documentadas en `docs/fase4-motor.md`)
//!
//! - **Corrected = uncorrected**: en tierra a ISA la corrección por
//!   temperatura/presión es ≈1; escribimos el mismo valor en ambas.
//! - **Sin FADEC fino**: N1 y el empuje se derivan linealmente de N2 (solo
//!   régimen de idle en tierra; no hay palanca en este slice).
//! - **Sin gate de bleed**: v1 arranca con master + modo IGN/START sin exigir
//!   presión neumática (el gate llega en el slice 5). `Restarting` y el modo
//!   CRANK (motoring sin ignición) quedan fuera de alcance.

use std::time::Duration;

use systems::pneumatic::{EngineModeSelector, EngineState};

use crate::variables::VariableStore;

/// LVAR del selector de modo de motor — **un único selector para ambos
/// motores** (`a320_systems/src/pneumatic.rs:1608-1609`). Valores del enum
/// `EngineModeSelector` (`fbw-common/.../systems/src/pneumatic/mod.rs:764-782`):
/// 0 = CRANK, 1 = NORM, 2 = IGN/START.
pub const MODE_SELECTOR_LVAR: &str = "TURB ENG IGNITION SWITCH EX1:1";

/// LVAR del engine master, **nuestro** (no existe en el Rust del vendor: en
/// MSFS el master vive en el fuel system C++). Ver D-020.
pub fn master_lvar(number: usize) -> String {
    format!("ENG_MASTER_{number}")
}

/// Interpreta el valor crudo del selector de modo.
///
/// Tolerante a valores fuera del enum para que una escritura cruda inválida no
/// haga panic en *nuestro* código: valores ≥ 3 caen en NORM (el `From<f64>`
/// del vendor haría panic al leer el mismo LVAR, pero ese es su contrato, no
/// el nuestro); negativos y NaN saturan a 0 = CRANK en el `as u8`, igual que
/// en el cast del vendor. En cualquier caso, todo lo que no sea exactamente
/// IGN/START (2) se comporta como "no arrancar".
fn mode_from(value: f64) -> EngineModeSelector {
    match value as u8 {
        0 => EngineModeSelector::Crank,
        2 => EngineModeSelector::Ignition,
        _ => EngineModeSelector::Norm,
    }
}

/// Modelo de un motor: máquina de estados + spool de N2 de primer orden.
///
/// Por tick ([`EngineModel::update`]): lee sus inputs de cabina del store
/// (master propio + selector compartido), transiciona el estado, integra N2
/// con la discretización exacta del primer orden
/// (`n2 += (target - n2) * (1 - exp(-dt/tau))`) y escribe la tabla completa de
/// simvars de motor ([`EngineModel::simvar_writes`], auditable al estilo de
/// `Environment`). Determinista: mismo `dt` y mismos inputs ⇒ mismos outputs.
pub struct EngineModel {
    /// Número de motor (1 o 2); solo para diagnósticos.
    number: usize,
    // Claves precomputadas (los simvars van indexados por motor).
    master_key: String,
    state_key: String,
    n2_key: String,
    corrected_n2_key: String,
    corrected_n1_key: String,
    thrust_key: String,
    starter_key: String,
    /// Estado FADEC del motor. Reutilizamos el enum del vendor para que los
    /// valores escritos en `ENGINE_STATE:{n}` no puedan divergir de los que
    /// sus consumidores esperan. `Restarting` no se produce nunca.
    state: EngineState,
    /// N2 sin corregir, en percent (0..~58.5 en este slice).
    n2_percent: f64,
}

impl EngineModel {
    // --- Constantes del spool (tramos del primer orden) ----------------------
    //
    // Elegidas para un arranque total de ~50 s (motoring ~14 s + aceleración
    // ~35 s), del orden del CFM LEAP real y verificado por los tests de timing
    // (`tests/engine_start.rs` exige idle en 40-70 s).

    /// N2 de light-off: por debajo, el starter hace motoring; a partir de aquí
    /// hay combustión y el motor acelera por sí mismo. Coincide con el cruce de
    /// 18 psi de presión de aceite del vendor (`leap_engine.rs:67-68`).
    const LIGHT_OFF_N2_PERCENT: f64 = 25.0;
    /// Target del tramo de motoring. Por encima del light-off (25 %) a
    /// propósito: un primer orden nunca alcanza su target, así que apuntar
    /// exactamente a 25 dejaría el arranque clavado en la asíntota.
    const MOTORING_TARGET_N2_PERCENT: f64 = 30.0;
    /// Tau del motoring: 25 % se cruza en `8·ln(30/5) ≈ 14.3 s`.
    const MOTORING_TAU_S: f64 = 8.0;
    /// Target del tramo de aceleración, un pelo por encima del idle por el
    /// mismo motivo asintótico; 58 % se cruza en `10·ln(34) ≈ 35.3 s`.
    const ACCEL_TARGET_N2_PERCENT: f64 = 59.0;
    /// Tau de la aceleración tras light-off.
    const ACCEL_TAU_S: f64 = 10.0;
    /// N2 al que el arranque se da por completado (`Starting → On`).
    const START_COMPLETE_N2_PERCENT: f64 = 58.0;
    /// N2 de idle en tierra (target estacionario del estado `On`).
    const IDLE_N2_PERCENT: f64 = 58.5;
    /// Tau del asentamiento fino hacia el idle una vez `On`.
    const IDLE_TAU_S: f64 = 4.0;
    /// Tau del spool-down (`Shutting`/`Off`, target 0): <1 % en `12·ln(58.5)
    /// ≈ 49 s`.
    const SPOOL_DOWN_TAU_S: f64 = 12.0;
    /// Umbral de parada: por debajo, `Shutting → Off`.
    const OFF_N2_PERCENT: f64 = 1.0;

    // --- Derivadas de N2 -----------------------------------------------------

    /// N1 de idle (percent); N1 se deriva linealmente: `n1 = n2 · 18.5/58.5`.
    const IDLE_N1_PERCENT: f64 = 18.5;
    /// Empuje de idle en libras; lineal en N2, 0 con el motor parado. El
    /// framework lee `TURB ENG JET THRUST` como `Mass` en **pounds**.
    const IDLE_THRUST_POUND: f64 = 1000.0;

    pub fn new(number: usize) -> Self {
        Self {
            number,
            master_key: master_lvar(number),
            state_key: format!("ENGINE_STATE:{number}"),
            n2_key: format!("ENGINE_N2:{number}"),
            corrected_n2_key: format!("TURB ENG CORRECTED N2:{number}"),
            corrected_n1_key: format!("TURB ENG CORRECTED N1:{number}"),
            thrust_key: format!("TURB ENG JET THRUST:{number}"),
            starter_key: format!("GENERAL ENG STARTER ACTIVE:{number}"),
            state: EngineState::Off,
            n2_percent: 0.0,
        }
    }

    /// Número de motor (1 o 2).
    pub fn number(&self) -> usize {
        self.number
    }

    /// Estado actual de la máquina de estados.
    pub fn state(&self) -> EngineState {
        self.state
    }

    /// N2 actual (percent, sin corregir).
    pub fn n2_percent(&self) -> f64 {
        self.n2_percent
    }

    /// Un tick del motor: leer inputs → transicionar → integrar N2 → escribir
    /// outputs. Se llama desde `Runtime::tick` **antes** de `simulation.tick`,
    /// de modo que el avión lee en este mismo tick lo que el motor genera.
    pub fn update(&mut self, delta: Duration, store: &mut VariableStore) {
        let master_on = store.read_by_name(&self.master_key) != 0.0;
        let mode = mode_from(store.read_by_name(MODE_SELECTOR_LVAR));

        self.transition(master_on, mode);
        self.integrate_n2(delta);

        for (name, value) in self.simvar_writes(master_on) {
            store.write_by_name(&name, value);
        }
    }

    /// Máquina de estados (v1, sin gate de bleed; `Restarting` fuera de
    /// alcance):
    ///
    /// - `Off → Starting`: master ON ∧ selector en IGN/START.
    /// - `Starting → On`: N2 ≥ 58 %.
    /// - `Starting|On → Shutting`: master OFF (cortar el master aborta también
    ///   un arranque en curso).
    /// - `Shutting → Off`: N2 < 1 %.
    ///
    /// Devolver el selector a NORM con el arranque en curso **no** lo aborta
    /// (como el FADEC real una vez secuenciado el arranque); solo el master
    /// corta.
    fn transition(&mut self, master_on: bool, mode: EngineModeSelector) {
        self.state = match self.state {
            EngineState::Off if master_on && mode == EngineModeSelector::Ignition => {
                EngineState::Starting
            }
            EngineState::Starting | EngineState::Restarting | EngineState::On if !master_on => {
                EngineState::Shutting
            }
            EngineState::Starting | EngineState::Restarting
                if self.n2_percent >= Self::START_COMPLETE_N2_PERCENT =>
            {
                EngineState::On
            }
            EngineState::Shutting if self.n2_percent < Self::OFF_N2_PERCENT => EngineState::Off,
            state => state,
        };
    }

    /// Integra el primer orden con su discretización exacta: para `target` y
    /// `tau` constantes durante el tick, `n2(t+dt)` coincide con la solución
    /// continua muestreada — el resultado depende de `dt`, no del número de
    /// ticks intermedios dentro de un tramo.
    fn integrate_n2(&mut self, delta: Duration) {
        let (target, tau_s) = match self.state {
            EngineState::Off | EngineState::Shutting => (0.0, Self::SPOOL_DOWN_TAU_S),
            EngineState::Starting | EngineState::Restarting
                if self.n2_percent < Self::LIGHT_OFF_N2_PERCENT =>
            {
                (Self::MOTORING_TARGET_N2_PERCENT, Self::MOTORING_TAU_S)
            }
            EngineState::Starting | EngineState::Restarting => {
                (Self::ACCEL_TARGET_N2_PERCENT, Self::ACCEL_TAU_S)
            }
            EngineState::On => (Self::IDLE_N2_PERCENT, Self::IDLE_TAU_S),
        };

        let alpha = 1.0 - (-delta.as_secs_f64() / tau_s).exp();
        self.n2_percent += (target - self.n2_percent) * alpha;
    }

    /// Pares (simvar, valor) que este motor escribe cada tick — la tabla
    /// completa y auditable de un vistazo, al estilo de
    /// `Environment::simvar_writes`.
    ///
    /// `GENERAL ENG STARTER ACTIVE:{n}` **espeja el master**, no el corte del
    /// starter: su único lector en el Rust del vendor es el controlador del PTU,
    /// que lo trata como *eng master on/off* (`hydraulic/mod.rs:3550-3554`), y
    /// el propio test bed del vendor lo deja a 1 mientras el motor corre
    /// (`hydraulic/mod.rs:7145-7183`). El corte del starter neumático a ~65 %
    /// ya lo modela el vendor con la válvula de arranque
    /// (`EngineStarterValveController`, `a320_systems/src/pneumatic.rs:458-473`),
    /// alimentada por nuestros `ENGINE_STATE`/`ENGINE_N2`.
    fn simvar_writes(&self, master_on: bool) -> Vec<(String, f64)> {
        let n1 = self.n2_percent * Self::IDLE_N1_PERCENT / Self::IDLE_N2_PERCENT;
        let thrust_lb = self.n2_percent * Self::IDLE_THRUST_POUND / Self::IDLE_N2_PERCENT;

        vec![
            (self.state_key.clone(), self.state as u8 as f64),
            // Uncorrected y corrected iguales (tierra, ISA — ver doc del módulo).
            (self.n2_key.clone(), self.n2_percent),
            (self.corrected_n2_key.clone(), self.n2_percent),
            (self.corrected_n1_key.clone(), n1),
            (self.thrust_key.clone(), thrust_lb),
            (self.starter_key.clone(), if master_on { 1.0 } else { 0.0 }),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: Duration = Duration::from_millis(200);

    /// Avanza `seconds` segundos en ticks de 200 ms.
    fn run(engine: &mut EngineModel, store: &mut VariableStore, seconds: f64) {
        let ticks = (seconds / DT.as_secs_f64()).round() as u64;
        for _ in 0..ticks {
            engine.update(DT, store);
        }
    }

    fn set_master(store: &mut VariableStore, number: usize, on: bool) {
        store.write_by_name(&master_lvar(number), if on { 1.0 } else { 0.0 });
    }

    fn set_mode(store: &mut VariableStore, mode: EngineModeSelector) {
        store.write_by_name(MODE_SELECTOR_LVAR, mode as u8 as f64);
    }

    #[test]
    fn off_and_untouched_nothing_moves() {
        let mut store = VariableStore::new();
        let mut engine = EngineModel::new(1);

        run(&mut engine, &mut store, 10.0);

        assert_eq!(engine.state(), EngineState::Off);
        assert_eq!(engine.n2_percent(), 0.0);
        assert_eq!(store.peek_by_name("ENGINE_STATE:1"), 0.0);
        assert_eq!(store.peek_by_name("ENGINE_N2:1"), 0.0);
        assert_eq!(store.peek_by_name("TURB ENG JET THRUST:1"), 0.0);
    }

    #[test]
    fn master_without_ignition_mode_does_not_start() {
        let mut store = VariableStore::new();
        let mut engine = EngineModel::new(1);
        set_mode(&mut store, EngineModeSelector::Norm);
        set_master(&mut store, 1, true);

        run(&mut engine, &mut store, 10.0);

        assert_eq!(engine.state(), EngineState::Off);
        assert_eq!(engine.n2_percent(), 0.0);
        // El master sí espeja en el simvar del PTU aunque no haya arranque.
        assert_eq!(store.peek_by_name("GENERAL ENG STARTER ACTIVE:1"), 1.0);
    }

    #[test]
    fn ignition_mode_without_master_does_not_start() {
        let mut store = VariableStore::new();
        let mut engine = EngineModel::new(1);
        set_mode(&mut store, EngineModeSelector::Ignition);

        run(&mut engine, &mut store, 10.0);

        assert_eq!(engine.state(), EngineState::Off);
        assert_eq!(engine.n2_percent(), 0.0);
        assert_eq!(store.peek_by_name("GENERAL ENG STARTER ACTIVE:1"), 0.0);
    }

    #[test]
    fn master_plus_ignition_walks_off_starting_on_and_reaches_idle() {
        let mut store = VariableStore::new();
        let mut engine = EngineModel::new(1);
        set_mode(&mut store, EngineModeSelector::Ignition);
        set_master(&mut store, 1, true);

        engine.update(DT, &mut store);
        assert_eq!(engine.state(), EngineState::Starting);
        assert_eq!(store.peek_by_name("ENGINE_STATE:1"), 2.0);

        // Motoring: 25 % se cruza en ~14.3 s (8·ln 6).
        run(&mut engine, &mut store, 20.0);
        assert!(
            engine.n2_percent() > EngineModel::LIGHT_OFF_N2_PERCENT,
            "a los 20 s el light-off (25 %) ya debería haberse cruzado, N2 = {}",
            engine.n2_percent()
        );
        assert_eq!(engine.state(), EngineState::Starting);

        // Aceleración: On (N2 ≥ 58) en ~50 s totales.
        run(&mut engine, &mut store, 40.0);
        assert_eq!(engine.state(), EngineState::On);
        // Y asentado en el idle.
        run(&mut engine, &mut store, 20.0);
        assert!(
            (engine.n2_percent() - EngineModel::IDLE_N2_PERCENT).abs() < 0.2,
            "N2 asentado = {}, se esperaba ~58.5",
            engine.n2_percent()
        );
        assert_eq!(store.peek_by_name("ENGINE_STATE:1"), 1.0);
    }

    #[test]
    fn n1_and_thrust_are_derived_linearly_from_n2() {
        let mut store = VariableStore::new();
        let mut engine = EngineModel::new(2);
        set_mode(&mut store, EngineModeSelector::Ignition);
        set_master(&mut store, 2, true);

        run(&mut engine, &mut store, 80.0); // idle asentado

        let n2 = store.peek_by_name("ENGINE_N2:2");
        let n1 = store.peek_by_name("TURB ENG CORRECTED N1:2");
        let thrust = store.peek_by_name("TURB ENG JET THRUST:2");
        // Corrected = uncorrected.
        assert_eq!(n2, store.peek_by_name("TURB ENG CORRECTED N2:2"));
        // Idle: N1 ~18.5 %, thrust ~1000 lb.
        assert!((n1 - 18.5).abs() < 0.2, "N1 = {n1}");
        assert!((thrust - 1000.0).abs() < 10.0, "thrust = {thrust}");
    }

    #[test]
    fn master_off_aborts_a_start_in_progress() {
        let mut store = VariableStore::new();
        let mut engine = EngineModel::new(1);
        set_mode(&mut store, EngineModeSelector::Ignition);
        set_master(&mut store, 1, true);
        run(&mut engine, &mut store, 10.0); // en pleno motoring

        set_master(&mut store, 1, false);
        engine.update(DT, &mut store);
        assert_eq!(engine.state(), EngineState::Shutting);
        assert_eq!(store.peek_by_name("GENERAL ENG STARTER ACTIVE:1"), 0.0);
    }

    #[test]
    fn shutdown_decays_monotonically_to_off() {
        let mut store = VariableStore::new();
        let mut engine = EngineModel::new(1);
        set_mode(&mut store, EngineModeSelector::Ignition);
        set_master(&mut store, 1, true);
        run(&mut engine, &mut store, 80.0);
        assert_eq!(engine.state(), EngineState::On);

        set_master(&mut store, 1, false);
        engine.update(DT, &mut store);
        assert_eq!(engine.state(), EngineState::Shutting);

        // Decae monótonamente y llega a Off (<1 %) en ~49 s (12·ln 58.5).
        let mut previous = engine.n2_percent();
        let mut elapsed = 0.0;
        while engine.state() != EngineState::Off {
            engine.update(DT, &mut store);
            assert!(
                engine.n2_percent() < previous,
                "el spool-down debe ser monótono"
            );
            previous = engine.n2_percent();
            elapsed += DT.as_secs_f64();
            assert!(elapsed < 90.0, "timeout del spool-down");
        }
        assert!(engine.n2_percent() < EngineModel::OFF_N2_PERCENT);
        assert_eq!(store.peek_by_name("ENGINE_STATE:1"), 0.0);
    }

    #[test]
    fn returning_the_selector_to_norm_does_not_abort_a_start() {
        let mut store = VariableStore::new();
        let mut engine = EngineModel::new(1);
        set_mode(&mut store, EngineModeSelector::Ignition);
        set_master(&mut store, 1, true);
        run(&mut engine, &mut store, 10.0);
        assert_eq!(engine.state(), EngineState::Starting);

        set_mode(&mut store, EngineModeSelector::Norm);
        run(&mut engine, &mut store, 60.0);
        assert_eq!(engine.state(), EngineState::On, "el arranque debe seguir");
    }

    #[test]
    fn the_integration_matches_the_exact_first_order_solution() {
        // Con target/tau constantes, la discretización es exacta: N ticks de dt
        // dan lo mismo que la solución continua en t = N·dt. Motoring desde 0:
        // n2(t) = 30·(1 - e^{-t/8}).
        let mut store = VariableStore::new();
        let mut engine = EngineModel::new(1);
        set_mode(&mut store, EngineModeSelector::Ignition);
        set_master(&mut store, 1, true);

        run(&mut engine, &mut store, 8.0); // aún < 25 %: un solo tramo
        let expected = 30.0 * (1.0 - (-8.0f64 / 8.0).exp());
        assert!(
            (engine.n2_percent() - expected).abs() < 1e-9,
            "N2 = {}, esperado {expected}",
            engine.n2_percent()
        );
    }

    #[test]
    fn simvar_writes_covers_the_whole_engine_table() {
        let mut store = VariableStore::new();
        let mut engine = EngineModel::new(1);
        engine.update(DT, &mut store);

        for key in [
            "ENGINE_STATE:1",
            "ENGINE_N2:1",
            "TURB ENG CORRECTED N2:1",
            "TURB ENG CORRECTED N1:1",
            "TURB ENG JET THRUST:1",
            "GENERAL ENG STARTER ACTIVE:1",
        ] {
            assert!(
                store.registry.find(key).is_some(),
                "simvar de motor no escrita cada tick: {key}"
            );
        }
    }

    #[test]
    fn mode_parsing_is_tolerant_and_defaults_to_norm() {
        assert_eq!(mode_from(0.0), EngineModeSelector::Crank);
        assert_eq!(mode_from(1.0), EngineModeSelector::Norm);
        assert_eq!(mode_from(2.0), EngineModeSelector::Ignition);
        // Valores fuera del enum no hacen panic en nuestro lado.
        assert_eq!(mode_from(7.0), EngineModeSelector::Norm);
    }
}
