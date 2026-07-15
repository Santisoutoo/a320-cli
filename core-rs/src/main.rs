//! Spike de Fase 0: instancia el A320 completo de FBW headless, lo avanza en
//! tierra y demuestra lectura/escritura de variables + inyección de un failure.
//!
//! Criterio de éxito (CLAUDE.md): avión instanciado, 1+ s simulados con
//! contexto de avión en tierra, y lectura de variables del sistema eléctrico.

use std::time::Duration;

use a320_systems::A320;
use systems::failures::FailureType;
use systems::simulation::test::{ReadByName, SimulationTestBed, TestBed, WriteByName};
use systems::simulation::StartState;
use uom::si::{f64::*, length::foot, thermodynamic_temperature::degree_celsius, velocity::knot};

/// Envoltorio del test bed público de FBW con el avión completo. Implementar
/// `TestBed` desbloquea los métodos de run/set_* y los blanket impls de
/// `ReadByName`/`WriteByName` (mismo patrón que los test beds de FBW).
struct SpikeBed {
    test_bed: SimulationTestBed<A320>,
}

impl TestBed for SpikeBed {
    type Aircraft = A320;

    fn test_bed(&self) -> &SimulationTestBed<A320> {
        &self.test_bed
    }

    fn test_bed_mut(&mut self) -> &mut SimulationTestBed<A320> {
        &mut self.test_bed
    }
}

impl SpikeBed {
    fn new() -> Self {
        let mut bed = Self {
            test_bed: SimulationTestBed::new_with_start_state(StartState::Apron, A320::new),
        };

        // Mundo exterior: en tierra, condiciones estándar (los defaults del
        // test bed son de vuelo: 250 kt / 5000 ft).
        bed.set_on_ground(true);
        bed.set_pressure_altitude(Length::new::<foot>(0.));
        bed.set_ambient_temperature(ThermodynamicTemperature::new::<degree_celsius>(15.));
        bed.set_indicated_airspeed(Velocity::new::<knot>(0.));
        bed
    }

    fn report(&mut self, label: &str) {
        let ac_1: bool = self.read_by_name("ELEC_AC_1_BUS_IS_POWERED");
        let dc_bat: bool = self.read_by_name("ELEC_DC_BAT_BUS_IS_POWERED");
        let dc_1: bool = self.read_by_name("ELEC_DC_1_BUS_IS_POWERED");
        let dc_hot_1: bool = self.read_by_name("ELEC_DC_HOT_1_BUS_IS_POWERED");
        let tr_1_ok: bool = self.read_by_name("ELEC_TR_1_POTENTIAL_NORMAL");
        println!(
            "{label:<16} AC_1={ac_1:<5} DC_1={dc_1:<5} DC_BAT={dc_bat:<5} DC_HOT_1={dc_hot_1:<5} TR_1_OK={tr_1_ok}"
        );
    }
}

fn main() {
    let mut bed = SpikeBed::new();

    // Cold & dark explícito: baterías OFF (el seed inicial de FBW deja los
    // pulsadores en su estado programado, que puede ser AUTO).
    bed.write_by_name("OVHD_ELEC_BAT_1_PB_IS_AUTO", false);
    bed.write_by_name("OVHD_ELEC_BAT_2_PB_IS_AUTO", false);
    bed.run_with_delta(Duration::from_secs(1));
    bed.report("[cold & dark]");

    // Baterías ON: el DC BAT bus debe cobrar vida (solo baterías: sin AC).
    bed.write_by_name("OVHD_ELEC_BAT_1_PB_IS_AUTO", true);
    bed.write_by_name("OVHD_ELEC_BAT_2_PB_IS_AUTO", true);
    bed.run_iterations_with_delta(10, Duration::from_millis(200));
    bed.report("[baterias ON]");

    // Ext pwr conectada y ON: la red AC completa debe alimentarse.
    bed.write_by_name("EXT_PWR_AVAIL:1", true);
    bed.write_by_name("OVHD_ELEC_EXT_PWR_PB_IS_ON", true);
    bed.run_iterations_with_delta(10, Duration::from_millis(200));
    bed.report("[ext pwr ON]");

    // Bonus (adelanto de Fase 2): failure del TR 1. La red se reconfigura
    // (el DC 1 pasa a alimentarse via bus tie, como en el avion real), pero
    // el propio TR 1 deja de dar potencial normal.
    bed.fail(FailureType::TransformerRectifier(1));
    bed.run_iterations_with_delta(10, Duration::from_millis(200));
    bed.report("[fail TR 1]");
}
