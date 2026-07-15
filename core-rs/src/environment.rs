//! El borde con el "mundo": los simvars que consume `UpdateContext`.
//!
//! `UpdateContext` los relee del reader **en cada tick**
//! (`update_context.rs:566-656`). Headless, nadie los escribe salvo nosotros;
//! sin ellos los sistemas reciben ceros — o, peor, defaults que significan algo
//! incorrecto en silencio (un `AMBIENT PRESSURE = 0` produce NaN y un `clamp`
//! interno hace panic; los defaults del test bed son de vuelo, 250 kt / 5000 ft).
//!
//! Este módulo encapsula el conjunto completo de simvars de mundo (nombres
//! exactos = constantes `*_KEY` de `update_context.rs:287-327`, tabuladas en
//! `docs/fase1-runtime.md`) y garantiza su **coherencia**: en tierra ⇒
//! `SIM ON GROUND = 1` ∧ `PLANE ALT ABOVE GROUND ≈ 0`, con presión/temperatura
//! sensatas juntas. El perfil por defecto es cold & dark en tierra, sin que el
//! caller tenga que configurar nada.
//!
//! Unidades (fáciles de equivocar, silenciosas si se equivocan):
//! - `AMBIENT PRESSURE` → **inHg**
//! - `AMBIENT TEMPERATURE` → **°C**
//! - `AMBIENT DENSITY` → **kg/m³**
//! - `AIRSPEED INDICATED` / `AIRSPEED TRUE` / `GPS GROUND SPEED` → **kt** (el
//!   framework lee GPS GROUND SPEED como Velocity; en tierra estacionario = 0)
//! - `PRESSURE ALTITUDE` / `PLANE ALT ABOVE GROUND` → **ft**

use uom::si::{
    f64::{Length, MassDensity, Pressure, ThermodynamicTemperature, Velocity},
    length::foot,
    mass_density::kilogram_per_cubic_meter,
    pressure::{hectopascal, inch_of_mercury},
    thermodynamic_temperature::degree_celsius,
    velocity::knot,
};

use crate::variables::VariableStore;

/// Nombres exactos de los simvars de mundo (constantes `*_KEY` de FBW,
/// `update_context.rs:287-327`). Se agrupan aquí para que la tabla escrita cada
/// tick sea auditable de un vistazo.
pub mod keys {
    // Estado del sim
    pub const IS_READY: &str = "IS_READY";
    pub const AIRCRAFT_PRESET_QUICK_MODE: &str = "AIRCRAFT_PRESET_QUICK_MODE";
    // Velocidades
    pub const AIRSPEED_INDICATED: &str = "AIRSPEED INDICATED";
    pub const AIRSPEED_TRUE: &str = "AIRSPEED TRUE";
    pub const GPS_GROUND_SPEED: &str = "GPS GROUND SPEED";
    pub const AIRSPEED_MACH: &str = "AIRSPEED MACH";
    pub const VELOCITY_WORLD_Y: &str = "VELOCITY WORLD Y";
    pub const VELOCITY_BODY_X: &str = "VELOCITY BODY X";
    pub const VELOCITY_BODY_Y: &str = "VELOCITY BODY Y";
    pub const VELOCITY_BODY_Z: &str = "VELOCITY BODY Z";
    // Posición / actitud
    pub const PRESSURE_ALTITUDE: &str = "PRESSURE ALTITUDE";
    pub const PLANE_ALT_ABOVE_GROUND: &str = "PLANE ALT ABOVE GROUND";
    pub const PLANE_LATITUDE: &str = "PLANE LATITUDE";
    pub const PLANE_PITCH_DEGREES: &str = "PLANE PITCH DEGREES";
    pub const PLANE_BANK_DEGREES: &str = "PLANE BANK DEGREES";
    pub const PLANE_HEADING_DEGREES_TRUE: &str = "PLANE HEADING DEGREES TRUE";
    // Ambiente
    pub const AMBIENT_PRESSURE: &str = "AMBIENT PRESSURE";
    pub const AMBIENT_TEMPERATURE: &str = "AMBIENT TEMPERATURE";
    pub const AMBIENT_DENSITY: &str = "AMBIENT DENSITY";
    pub const AMBIENT_WIND_X: &str = "AMBIENT WIND X";
    pub const AMBIENT_WIND_Y: &str = "AMBIENT WIND Y";
    pub const AMBIENT_WIND_Z: &str = "AMBIENT WIND Z";
    pub const AMBIENT_PRECIP_RATE: &str = "AMBIENT PRECIP RATE";
    pub const AMBIENT_IN_CLOUD: &str = "AMBIENT IN CLOUD";
    pub const SURFACE_TYPE: &str = "SURFACE TYPE";
    // Tierra
    pub const SIM_ON_GROUND: &str = "SIM ON GROUND";
    // Aceleraciones
    pub const ACCELERATION_BODY_X: &str = "ACCELERATION BODY X";
    pub const ACCELERATION_BODY_Y: &str = "ACCELERATION BODY Y";
    pub const ACCELERATION_BODY_Z_WITH_REVERSER: &str = "ACCELERATION_BODY_Z_WITH_REVERSER";
    pub const ROTATION_ACCELERATION_BODY_X: &str = "ROTATION ACCELERATION BODY X";
    pub const ROTATION_ACCELERATION_BODY_Y: &str = "ROTATION ACCELERATION BODY Y";
    pub const ROTATION_ACCELERATION_BODY_Z: &str = "ROTATION ACCELERATION BODY Z";
    pub const ROTATION_VELOCITY_BODY_X: &str = "ROTATION VELOCITY BODY X";
    pub const ROTATION_VELOCITY_BODY_Y: &str = "ROTATION VELOCITY BODY Y";
    pub const ROTATION_VELOCITY_BODY_Z: &str = "ROTATION VELOCITY BODY Z";
    // Masas
    pub const TOTAL_WEIGHT: &str = "TOTAL WEIGHT";
    pub const TOTAL_WEIGHT_YAW_MOI: &str = "TOTAL WEIGHT YAW MOI";
    pub const TOTAL_WEIGHT_PITCH_MOI: &str = "TOTAL WEIGHT PITCH MOI";
}

/// Estado del "mundo exterior" que se escribe en el store en cada tick.
///
/// Guarda los pocos parámetros de alto nivel con tipos `uom` (para que las
/// unidades no puedan equivocarse) y deriva de ellos, de forma coherente, el
/// resto de la tabla. Fase 1 es solo tierra: las variables ligadas a motores
/// (N1/N2) y la dinámica de vuelo quedan fuera (Fase 4, #18).
#[derive(Debug, Clone, Copy)]
pub struct Environment {
    /// En tierra ⇒ `SIM ON GROUND = 1` y `PLANE ALT ABOVE GROUND = 0`.
    on_ground: bool,
    /// Elevación del campo (= `PRESSURE ALTITUDE`).
    field_elevation: Length,
    /// IAS. En tierra estacionario, 0.
    indicated_airspeed: Velocity,
    /// OAT.
    ambient_temperature: ThermodynamicTemperature,
    /// Presión estática ambiente en el avión (se escribe en inHg).
    ambient_pressure: Pressure,
    /// Densidad del aire.
    ambient_density: MassDensity,
}

impl Default for Environment {
    /// Perfil cold & dark en tierra a nivel del mar, sin setup del caller.
    fn default() -> Self {
        Self {
            on_ground: true,
            field_elevation: Length::new::<foot>(0.0),
            indicated_airspeed: Velocity::new::<knot>(0.0),
            ambient_temperature: ThermodynamicTemperature::new::<degree_celsius>(15.0),
            ambient_pressure: Pressure::new::<inch_of_mercury>(29.92),
            ambient_density: MassDensity::new::<kilogram_per_cubic_meter>(1.225),
        }
    }
}

impl Environment {
    /// Perfil cold & dark en tierra (alias explícito de `Default`).
    pub fn cold_and_dark_ground() -> Self {
        Self::default()
    }

    /// Fija los knobs de alto nivel del contrato de la API y deriva el resto de
    /// forma coherente.
    ///
    /// - `altitude_ft`: elevación del campo (Fase 1: en tierra, es la cota).
    /// - `indicated_airspeed_kt`: IAS.
    /// - `oat_celsius`: temperatura exterior (°C).
    /// - `qnh_hpa`: reglaje de presión (hPa); se convierte a inHg para el simvar.
    ///
    /// Fase 1 es solo tierra, así que `on_ground` se mantiene en `true` y la
    /// coherencia (SIM ON GROUND / AGL≈0) la garantiza [`Environment::write_all`].
    pub fn set(
        &mut self,
        altitude_ft: f64,
        indicated_airspeed_kt: f64,
        oat_celsius: f64,
        qnh_hpa: f64,
    ) {
        self.field_elevation = Length::new::<foot>(altitude_ft);
        self.indicated_airspeed = Velocity::new::<knot>(indicated_airspeed_kt);
        self.ambient_temperature = ThermodynamicTemperature::new::<degree_celsius>(oat_celsius);
        self.ambient_pressure = Pressure::new::<hectopascal>(qnh_hpa);
    }

    pub fn on_ground(&self) -> bool {
        self.on_ground
    }

    /// Pares (simvar, valor) que se escriben cada tick. Es la tabla completa de
    /// `docs/fase1-runtime.md`; tenerla como una única lista la hace auditable
    /// (test: cubre todas las claves de [`keys`]).
    ///
    /// Coherencia: `SIM ON GROUND` y `PLANE ALT ABOVE GROUND` se derivan de
    /// `on_ground`, de modo que no es representable "en tierra a 5000 ft AGL".
    fn simvar_writes(&self) -> Vec<(&'static str, f64)> {
        let ias_kt = self.indicated_airspeed.get::<knot>();
        let alt_ft = self.field_elevation.get::<foot>();
        let (on_ground_flag, alt_above_ground_ft) = if self.on_ground {
            (1.0, 0.0)
        } else {
            (0.0, alt_ft)
        };

        vec![
            // Estado del sim
            (keys::IS_READY, 1.0),
            (keys::AIRCRAFT_PRESET_QUICK_MODE, 0.0),
            // Velocidades (en tierra estacionario, todas 0)
            (keys::AIRSPEED_INDICATED, ias_kt),
            (keys::AIRSPEED_TRUE, ias_kt),
            (keys::GPS_GROUND_SPEED, 0.0),
            (keys::AIRSPEED_MACH, 0.0),
            (keys::VELOCITY_WORLD_Y, 0.0),
            (keys::VELOCITY_BODY_X, 0.0),
            (keys::VELOCITY_BODY_Y, 0.0),
            (keys::VELOCITY_BODY_Z, 0.0),
            // Posición / actitud
            (keys::PRESSURE_ALTITUDE, alt_ft),
            (keys::PLANE_ALT_ABOVE_GROUND, alt_above_ground_ft),
            (keys::PLANE_LATITUDE, 0.0),
            (keys::PLANE_PITCH_DEGREES, 0.0),
            (keys::PLANE_BANK_DEGREES, 0.0),
            (keys::PLANE_HEADING_DEGREES_TRUE, 0.0),
            // Ambiente (¡AMBIENT PRESSURE en inHg!)
            (
                keys::AMBIENT_PRESSURE,
                self.ambient_pressure.get::<inch_of_mercury>(),
            ),
            (
                keys::AMBIENT_TEMPERATURE,
                self.ambient_temperature.get::<degree_celsius>(),
            ),
            (
                keys::AMBIENT_DENSITY,
                self.ambient_density.get::<kilogram_per_cubic_meter>(),
            ),
            (keys::AMBIENT_WIND_X, 0.0),
            (keys::AMBIENT_WIND_Y, 0.0),
            (keys::AMBIENT_WIND_Z, 0.0),
            (keys::AMBIENT_PRECIP_RATE, 0.0),
            (keys::AMBIENT_IN_CLOUD, 0.0),
            (keys::SURFACE_TYPE, 0.0),
            // Tierra (coherente con on_ground)
            (keys::SIM_ON_GROUND, on_ground_flag),
            // Aceleraciones (estacionario)
            (keys::ACCELERATION_BODY_X, 0.0),
            (keys::ACCELERATION_BODY_Y, 0.0),
            (keys::ACCELERATION_BODY_Z_WITH_REVERSER, 0.0),
            (keys::ROTATION_ACCELERATION_BODY_X, 0.0),
            (keys::ROTATION_ACCELERATION_BODY_Y, 0.0),
            (keys::ROTATION_ACCELERATION_BODY_Z, 0.0),
            (keys::ROTATION_VELOCITY_BODY_X, 0.0),
            (keys::ROTATION_VELOCITY_BODY_Y, 0.0),
            (keys::ROTATION_VELOCITY_BODY_Z, 0.0),
            // Masas (Fase 4: masas/inercias realistas con motores; en tierra
            // eléctrico 0 reproduce el baseline probado del test bed)
            (keys::TOTAL_WEIGHT, 0.0),
            (keys::TOTAL_WEIGHT_YAW_MOI, 0.0),
            (keys::TOTAL_WEIGHT_PITCH_MOI, 0.0),
        ]
    }

    /// Escribe la tabla completa de simvars de mundo en el store.
    /// Se llama en cada tick antes de `simulation.tick`.
    pub fn write_all(&self, store: &mut VariableStore) {
        for (name, value) in self.simvar_writes() {
            store.write_by_name(name, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Todas las claves declaradas en `keys`, para comprobar cobertura.
    const ALL_KEYS: &[&str] = &[
        keys::IS_READY,
        keys::AIRCRAFT_PRESET_QUICK_MODE,
        keys::AIRSPEED_INDICATED,
        keys::AIRSPEED_TRUE,
        keys::GPS_GROUND_SPEED,
        keys::AIRSPEED_MACH,
        keys::VELOCITY_WORLD_Y,
        keys::VELOCITY_BODY_X,
        keys::VELOCITY_BODY_Y,
        keys::VELOCITY_BODY_Z,
        keys::PRESSURE_ALTITUDE,
        keys::PLANE_ALT_ABOVE_GROUND,
        keys::PLANE_LATITUDE,
        keys::PLANE_PITCH_DEGREES,
        keys::PLANE_BANK_DEGREES,
        keys::PLANE_HEADING_DEGREES_TRUE,
        keys::AMBIENT_PRESSURE,
        keys::AMBIENT_TEMPERATURE,
        keys::AMBIENT_DENSITY,
        keys::AMBIENT_WIND_X,
        keys::AMBIENT_WIND_Y,
        keys::AMBIENT_WIND_Z,
        keys::AMBIENT_PRECIP_RATE,
        keys::AMBIENT_IN_CLOUD,
        keys::SURFACE_TYPE,
        keys::SIM_ON_GROUND,
        keys::ACCELERATION_BODY_X,
        keys::ACCELERATION_BODY_Y,
        keys::ACCELERATION_BODY_Z_WITH_REVERSER,
        keys::ROTATION_ACCELERATION_BODY_X,
        keys::ROTATION_ACCELERATION_BODY_Y,
        keys::ROTATION_ACCELERATION_BODY_Z,
        keys::ROTATION_VELOCITY_BODY_X,
        keys::ROTATION_VELOCITY_BODY_Y,
        keys::ROTATION_VELOCITY_BODY_Z,
        keys::TOTAL_WEIGHT,
        keys::TOTAL_WEIGHT_YAW_MOI,
        keys::TOTAL_WEIGHT_PITCH_MOI,
    ];

    #[test]
    fn write_all_covers_the_whole_table() {
        let env = Environment::default();
        let mut store = VariableStore::new();
        env.write_all(&mut store);

        for key in ALL_KEYS {
            assert!(
                store.registry.find(key).is_some(),
                "simvar not written each tick: {key}"
            );
        }
        // Y nada más allá de la tabla (todas las escritas están en ALL_KEYS).
        assert_eq!(store.list_variables().len(), ALL_KEYS.len());
    }

    #[test]
    fn default_profile_is_stationary_on_the_ground() {
        let env = Environment::default();
        let mut store = VariableStore::new();
        env.write_all(&mut store);

        assert_eq!(store.peek_by_name(keys::SIM_ON_GROUND), 1.0);
        assert_eq!(store.peek_by_name(keys::AIRSPEED_INDICATED), 0.0);
        assert_eq!(store.peek_by_name(keys::AIRSPEED_TRUE), 0.0);
        assert_eq!(store.peek_by_name(keys::GPS_GROUND_SPEED), 0.0);
        assert_eq!(store.peek_by_name(keys::PLANE_ALT_ABOVE_GROUND), 0.0);
    }

    #[test]
    fn ambient_pressure_is_written_in_inhg() {
        let env = Environment::default();
        let mut store = VariableStore::new();
        env.write_all(&mut store);

        // Default 29.92 inHg — NO 1013.25 (hPa) ni 101325 (Pa).
        assert!(
            (store.peek_by_name(keys::AMBIENT_PRESSURE) - 29.92).abs() < 1e-6,
            "AMBIENT PRESSURE must be inHg, got {}",
            store.peek_by_name(keys::AMBIENT_PRESSURE)
        );
    }

    #[test]
    fn set_maps_qnh_hpa_to_ambient_pressure_inhg() {
        let mut env = Environment::default();
        env.set(0.0, 0.0, 15.0, 1013.25);

        let mut store = VariableStore::new();
        env.write_all(&mut store);

        // 1013.25 hPa == 29.9213 inHg.
        assert!(
            (store.peek_by_name(keys::AMBIENT_PRESSURE) - 29.9213).abs() < 1e-3,
            "1013.25 hPa should be ~29.92 inHg, got {}",
            store.peek_by_name(keys::AMBIENT_PRESSURE)
        );
    }

    #[test]
    fn set_writes_altitude_and_temperature() {
        let mut env = Environment::default();
        env.set(1500.0, 0.0, -5.0, 1013.25);

        let mut store = VariableStore::new();
        env.write_all(&mut store);

        assert_eq!(store.peek_by_name(keys::PRESSURE_ALTITUDE), 1500.0);
        assert_eq!(store.peek_by_name(keys::AMBIENT_TEMPERATURE), -5.0);
        // En tierra, AGL sigue coherente en 0 aunque la cota sea 1500 ft.
        assert_eq!(store.peek_by_name(keys::PLANE_ALT_ABOVE_GROUND), 0.0);
        assert_eq!(store.peek_by_name(keys::SIM_ON_GROUND), 1.0);
    }
}
