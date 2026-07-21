//! Catálogo **curado a mano** de fallos inyectables.
//!
//! El mecanismo de fallos de FBW es un enum Rust (`FailureType`) que no puede
//! salir tal cual hacia Python ni hacia el MCP: no deriva `Debug` ni
//! `Serialize`, no tiene representación numérica propia, y su forma cambia
//! cuando movemos el pin del vendor. El benchmark necesita lo contrario —
//! identificadores **estables, serializables y legibles** que puedan escribirse
//! en un fichero de escenario y sigan significando lo mismo dentro de un año.
//!
//! Por eso el mapeo id→`FailureType` es **nuestro y versionado**: un bump del
//! pin se convierte en un diff visible aquí (o en un fallo de compilación si
//! una variante desaparece), en vez de en una renumeración silenciosa.
//!
//! **Alcance: ATA24 (eléctrico, Fase 2) + ATA29 (hidráulico, Fase 4 slice 1).**
//! Los grupos de neumático/fuel/tren se añaden en su fase, cuando exista cómo
//! verificar que el fallo hace algo observable; catalogar un id que ningún test
//! puede ejercitar es catalogar un id que puede estar mal mapeado sin que nadie
//! se entere.
//!
//! **Los ids ATA vienen de FBW.** La tabla `(u32, FailureType)` de
//! `a320_systems_wasm/src/lib.rs:101-200` es la numeración que usa el propio
//! FBW de cara a su UI. La copiamos como campo [`FailureDef::ata`] para poder
//! cruzar cualquier id nuestro con upstream. Es un **dato copiado**, no un
//! enlace: `a320_systems_wasm` no entra en el build nativo (D-005).
//!
//! **No existe fallo de batería ni de contactor** en todo el enum de FBW
//! (`battery.rs` no tiene campo `Failure`). Los únicos componentes eléctricos
//! fallables son generadores, TRs, static inverter y buses. Si un escenario
//! necesita "pérdida de batería", el proxy más cercano es `elec.bus.dc_bat`
//! (des-energizar el bus), no un fallo nativo de la batería: no lo hay.

use std::fmt;

use systems::failures::FailureType;
use systems::shared::{
    AirbusElectricPumpId, AirbusEngineDrivenPumpId, ElectricalBusType, HydraulicColor,
};

/// Sistema al que pertenece el fallo (para agrupar en CLI/MCP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureGroup {
    /// Sistema eléctrico (ATA24).
    Elec,
    /// Sistema hidráulico (ATA29).
    Hyd,
    /// Unidad de potencia auxiliar (ATA49).
    ///
    /// FBW no cataloga **ningún** fallo inyectable ATA49 (la tabla de
    /// `a320_systems_wasm/src/lib.rs` no tiene entradas 49_xxx; el único fallo
    /// relacionado con el APU es su generador, clasificado ATA24). El grupo
    /// existe porque las reglas ECAM del slice 2 de Fase 4 (#56) necesitan
    /// declarar su sistema, no porque haya `FailureDef`s que agrupar.
    Apu,
    // Fuel, Engines, Brakes, RA... se añaden en fases posteriores.
}

impl FailureGroup {
    pub fn as_str(&self) -> &'static str {
        match self {
            FailureGroup::Elec => "ELEC",
            FailureGroup::Hyd => "HYD",
            FailureGroup::Apu => "APU",
        }
    }

    /// Rango ATA (inicio inclusivo, fin exclusivo) que FBW asigna al grupo.
    pub fn ata_range(&self) -> std::ops::Range<u32> {
        match self {
            FailureGroup::Elec => 24_000..25_000,
            FailureGroup::Hyd => 29_000..30_000,
            FailureGroup::Apu => 49_000..50_000,
        }
    }
}

/// Una entrada del catálogo: un fallo inyectable con sus metadatos.
///
/// `Copy` porque todos los campos lo son (`FailureType` deriva `Copy`), igual
/// que [`crate::controls::Control`]. `Debug` va a mano y no derivado: el
/// `FailureType` de FBW **no implementa `Debug`** (solo `Clone, Copy, PartialEq,
/// Eq, Hash`), así que no hay forma de formatearlo. Tampoco se pierde nada: lo
/// legible es nuestro `id`, que es precisamente lo que el enum del vendor no
/// sabe decir de sí mismo.
#[derive(Clone, Copy)]
pub struct FailureDef {
    /// Identificador **estable** y jerárquico (lo que se teclea, lo que se le
    /// da al LLM y lo que se escribe en un escenario). Nuestro, no de FBW.
    pub id: &'static str,
    /// Id numérico ATA que usa FBW para el mismo fallo (para cruzar con
    /// upstream). Copiado de `a320_systems_wasm/src/lib.rs`.
    pub ata: u32,
    /// La variante del vendor a la que se inyecta.
    pub failure_type: FailureType,
    /// Descripción de una línea.
    pub description: &'static str,
    /// Grupo por sistema.
    pub group: FailureGroup,
}

impl fmt::Debug for FailureDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FailureDef")
            .field("id", &self.id)
            .field("ata", &self.ata)
            .field("description", &self.description)
            .field("group", &self.group)
            .finish_non_exhaustive() // `failure_type` no es formateable (FBW no deriva Debug)
    }
}

/// Catálogo curado. **Fase 2: ATA24 (eléctrico). Fase 4 slice 1: ATA29
/// (hidráulico).**
///
/// Correspondencia 1:1 con las tablas de FBW (`a320_systems_wasm/src/lib.rs`):
/// ATA24 en `:101-163` (20 entradas) y ATA29 en `:164-200` (13 entradas).
pub const CATALOG: &[FailureDef] = &[
    // --- Transformer rectifiers y static inverter ---
    FailureDef {
        id: "elec.tr.1",
        ata: 24_000,
        failure_type: FailureType::TransformerRectifier(1),
        description: "TR 1 fails: stops rectifying AC to DC (DC 1 re-feeds via the bus tie)",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.tr.2",
        ata: 24_001,
        failure_type: FailureType::TransformerRectifier(2),
        description: "TR 2 fails: stops rectifying AC to DC (DC 2 re-feeds via the bus tie)",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.tr.3",
        ata: 24_002,
        failure_type: FailureType::TransformerRectifier(3),
        description: "ESS TR fails: stops feeding the DC ESS bus",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.static_inverter",
        ata: 24_004,
        failure_type: FailureType::StaticInverter,
        description: "Static inverter fails: no AC from the battery in emergency configuration",
        group: FailureGroup::Elec,
    },
    // --- Generadores ---
    FailureDef {
        id: "elec.gen.1",
        ata: 24_020,
        failure_type: FailureType::Generator(1),
        description: "Engine 1 generator fails: stops supplying the AC network",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.gen.2",
        ata: 24_021,
        failure_type: FailureType::Generator(2),
        description: "Engine 2 generator fails: stops supplying the AC network",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.apu_gen.1",
        ata: 24_030,
        failure_type: FailureType::ApuGenerator(1),
        description: "APU generator fails: stops supplying the AC network (raises APU GEN FAULT)",
        group: FailureGroup::Elec,
    },
    // --- Buses AC ---
    FailureDef {
        id: "elec.bus.ac.1",
        ata: 24_100,
        failure_type: FailureType::ElectricalBus(ElectricalBusType::AlternatingCurrent(1)),
        description: "AC BUS 1 fails: the bus stops conducting and everything it feeds goes dark",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.bus.ac.2",
        ata: 24_101,
        failure_type: FailureType::ElectricalBus(ElectricalBusType::AlternatingCurrent(2)),
        description: "AC BUS 2 fails: the bus stops conducting and everything it feeds goes dark",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.bus.ac_ess",
        ata: 24_102,
        failure_type: FailureType::ElectricalBus(ElectricalBusType::AlternatingCurrentEssential),
        description: "AC ESS bus fails: the essential AC bus stops conducting",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.bus.ac_ess_shed",
        ata: 24_103,
        failure_type: FailureType::ElectricalBus(
            ElectricalBusType::AlternatingCurrentEssentialShed,
        ),
        description: "AC ESS SHED bus fails: the shed section of the essential AC bus stops conducting",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.bus.ac_static_inv",
        ata: 24_104,
        failure_type: FailureType::ElectricalBus(
            ElectricalBusType::AlternatingCurrentStaticInverter,
        ),
        description: "AC STAT INV bus fails: the static inverter output bus stops conducting",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.bus.ac_gnd_flt_svc",
        ata: 24_105,
        failure_type: FailureType::ElectricalBus(
            ElectricalBusType::AlternatingCurrentGndFltService,
        ),
        description: "AC GND/FLT SVC bus fails: the ground/flight service AC bus stops conducting",
        group: FailureGroup::Elec,
    },
    // --- Buses DC ---
    FailureDef {
        id: "elec.bus.dc.1",
        ata: 24_106,
        failure_type: FailureType::ElectricalBus(ElectricalBusType::DirectCurrent(1)),
        description: "DC BUS 1 fails: the bus stops conducting and everything it feeds goes dark",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.bus.dc.2",
        ata: 24_107,
        failure_type: FailureType::ElectricalBus(ElectricalBusType::DirectCurrent(2)),
        description: "DC BUS 2 fails: the bus stops conducting and everything it feeds goes dark",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.bus.dc_ess",
        ata: 24_108,
        failure_type: FailureType::ElectricalBus(ElectricalBusType::DirectCurrentEssential),
        description: "DC ESS bus fails: the essential DC bus stops conducting",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.bus.dc_ess_shed",
        ata: 24_109,
        failure_type: FailureType::ElectricalBus(ElectricalBusType::DirectCurrentEssentialShed),
        description: "DC ESS SHED bus fails: the shed section of the essential DC bus stops conducting",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.bus.dc_bat",
        ata: 24_110,
        failure_type: FailureType::ElectricalBus(ElectricalBusType::DirectCurrentBattery),
        description: "DC BAT bus fails: the battery bus stops conducting (closest proxy to a battery loss; FBW models no battery failure)",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.bus.dc_hot.1",
        ata: 24_111,
        failure_type: FailureType::ElectricalBus(ElectricalBusType::DirectCurrentHot(1)),
        description: "DC HOT BUS 1 fails: the permanently live bus 1 stops conducting",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.bus.dc_hot.2",
        ata: 24_112,
        failure_type: FailureType::ElectricalBus(ElectricalBusType::DirectCurrentHot(2)),
        description: "DC HOT BUS 2 fails: the permanently live bus 2 stops conducting",
        group: FailureGroup::Elec,
    },
    FailureDef {
        id: "elec.bus.dc_gnd_flt_svc",
        ata: 24_113,
        failure_type: FailureType::ElectricalBus(ElectricalBusType::DirectCurrentGndFltService),
        description: "DC GND/FLT SVC bus fails: the ground/flight service DC bus stops conducting",
        group: FailureGroup::Elec,
    },
    // --- Hidráulico (ATA29, Fase 4 slice 1) ----------------------------------
    // Correspondencia 1:1 con la tabla ATA29 de FBW
    // (`a320_systems_wasm/src/lib.rs:164-200`), 13 entradas.
    FailureDef {
        id: "hyd.reservoir_leak.green",
        ata: 29_000,
        failure_type: FailureType::ReservoirLeak(HydraulicColor::Green),
        description: "Green reservoir leak: fluid drains overboard (permanently lost) until the pumps starve and green pressure collapses",
        group: FailureGroup::Hyd,
    },
    FailureDef {
        id: "hyd.reservoir_leak.blue",
        ata: 29_001,
        failure_type: FailureType::ReservoirLeak(HydraulicColor::Blue),
        description: "Blue reservoir leak: fluid drains overboard (permanently lost) until the pumps starve and blue pressure collapses",
        group: FailureGroup::Hyd,
    },
    FailureDef {
        id: "hyd.reservoir_leak.yellow",
        ata: 29_002,
        failure_type: FailureType::ReservoirLeak(HydraulicColor::Yellow),
        description: "Yellow reservoir leak: fluid drains overboard (permanently lost) until the pumps starve and yellow pressure collapses",
        group: FailureGroup::Hyd,
    },
    FailureDef {
        id: "hyd.reservoir_air_leak.green",
        ata: 29_003,
        failure_type: FailureType::ReservoirAirLeak(HydraulicColor::Green),
        description: "Green reservoir air pressurisation leak: air pressure decays, degrading pump feed (cavitation)",
        group: FailureGroup::Hyd,
    },
    FailureDef {
        id: "hyd.reservoir_air_leak.blue",
        ata: 29_004,
        failure_type: FailureType::ReservoirAirLeak(HydraulicColor::Blue),
        description: "Blue reservoir air pressurisation leak: air pressure decays, degrading pump feed (cavitation)",
        group: FailureGroup::Hyd,
    },
    FailureDef {
        id: "hyd.reservoir_air_leak.yellow",
        ata: 29_005,
        failure_type: FailureType::ReservoirAirLeak(HydraulicColor::Yellow),
        description: "Yellow reservoir air pressurisation leak: air pressure decays, degrading pump feed (cavitation)",
        group: FailureGroup::Hyd,
    },
    FailureDef {
        id: "hyd.reservoir_return_leak.green",
        ata: 29_006,
        failure_type: FailureType::ReservoirReturnLeak(HydraulicColor::Green),
        description: "Green return line leak: fluid returning to the reservoir is lost instead of recovered",
        group: FailureGroup::Hyd,
    },
    FailureDef {
        id: "hyd.reservoir_return_leak.blue",
        ata: 29_007,
        failure_type: FailureType::ReservoirReturnLeak(HydraulicColor::Blue),
        description: "Blue return line leak: fluid returning to the reservoir is lost instead of recovered",
        group: FailureGroup::Hyd,
    },
    FailureDef {
        id: "hyd.reservoir_return_leak.yellow",
        ata: 29_008,
        failure_type: FailureType::ReservoirReturnLeak(HydraulicColor::Yellow),
        description: "Yellow return line leak: fluid returning to the reservoir is lost instead of recovered",
        group: FailureGroup::Hyd,
    },
    FailureDef {
        id: "hyd.eng_pump_overheat.green",
        ata: 29_009,
        failure_type: FailureType::EnginePumpOverheat(AirbusEngineDrivenPumpId::Green),
        description: "Engine 1 driven pump (green) overheats: raises the ENG 1 PUMP fault while the pump runs",
        group: FailureGroup::Hyd,
    },
    FailureDef {
        id: "hyd.elec_pump_overheat.blue",
        ata: 29_010,
        failure_type: FailureType::ElecPumpOverheat(AirbusElectricPumpId::Blue),
        description: "Blue electric pump overheats: raises the BLUE ELEC PUMP fault while the pump runs",
        group: FailureGroup::Hyd,
    },
    FailureDef {
        id: "hyd.eng_pump_overheat.yellow",
        ata: 29_011,
        failure_type: FailureType::EnginePumpOverheat(AirbusEngineDrivenPumpId::Yellow),
        description: "Engine 2 driven pump (yellow) overheats: raises the ENG 2 PUMP fault while the pump runs",
        group: FailureGroup::Hyd,
    },
    FailureDef {
        id: "hyd.elec_pump_overheat.yellow",
        ata: 29_012,
        failure_type: FailureType::ElecPumpOverheat(AirbusElectricPumpId::Yellow),
        description: "Yellow electric pump overheats: raises the YELLOW ELEC PUMP fault while the pump runs",
        group: FailureGroup::Hyd,
    },
];

/// Resuelve un id estable a su entrada del catálogo.
pub fn by_id(id: &str) -> Option<&'static FailureDef> {
    CATALOG.iter().find(|f| f.id == id)
}

/// Resuelve un `FailureType` del vendor de vuelta a su id estable.
///
/// Es la dirección inversa de [`by_id`], y la necesita `active_failures()`: el
/// runtime guarda `FailureType` (lo que el vendor entiende) pero la API solo
/// habla de ids nuestros.
pub fn id_of(failure_type: FailureType) -> Option<&'static str> {
    CATALOG
        .iter()
        .find(|f| f.failure_type == failure_type)
        .map(|f| f.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_the_phase2_electrical_failures() {
        // Los fallos que la Fase 2 necesita poder inyectar: el del spike (TR 1)
        // y el del demo de #16 (APU gen).
        for id in ["elec.tr.1", "elec.apu_gen.1", "elec.gen.1", "elec.bus.ac.1"] {
            assert!(by_id(id).is_some(), "falta el fallo '{id}'");
        }
    }

    #[test]
    fn ids_are_unique() {
        for (i, a) in CATALOG.iter().enumerate() {
            for b in &CATALOG[i + 1..] {
                assert_ne!(a.id, b.id, "id duplicado: {}", a.id);
            }
        }
    }

    #[test]
    fn failure_types_are_unique() {
        // Dos ids que apunten al mismo FailureType harían que limpiar uno
        // limpiara el otro (el set del vendor es por FailureType, no por id).
        for (i, a) in CATALOG.iter().enumerate() {
            for b in &CATALOG[i + 1..] {
                assert!(
                    a.failure_type != b.failure_type,
                    "'{}' y '{}' mapean al mismo FailureType",
                    a.id,
                    b.id
                );
            }
        }
    }

    #[test]
    fn ata_ids_are_unique_and_within_their_groups_range() {
        for (i, a) in CATALOG.iter().enumerate() {
            assert!(
                a.group.ata_range().contains(&a.ata),
                "'{}' tiene un ATA fuera del rango de su grupo {:?}: {}",
                a.id,
                a.group,
                a.ata
            );
            for b in &CATALOG[i + 1..] {
                assert_ne!(a.ata, b.ata, "ATA duplicado: {}", a.ata);
            }
        }
    }

    /// Anti-drift del issue #14: cada id del catálogo resuelve, y resuelve de
    /// vuelta a sí mismo. Si un bump del pin del vendor borrase o renombrase una
    /// variante de `FailureType`, este módulo dejaría de compilar — que es
    /// exactamente la señal que queremos (un fallo ruidoso, no un id que deja de
    /// significar nada en silencio).
    #[test]
    fn every_catalog_id_resolves_round_trip() {
        for f in CATALOG {
            let def = by_id(f.id).unwrap_or_else(|| panic!("'{}' no resuelve", f.id));
            assert_eq!(def.id, f.id);
            assert_eq!(
                id_of(f.failure_type),
                Some(f.id),
                "'{}' no resuelve de vuelta desde su FailureType",
                f.id
            );
        }
    }

    /// El prefijo del id jerárquico y el grupo deben contar la misma historia:
    /// un `hyd.*` clasificado como ELEC rompería el agrupado de CLI/MCP sin que
    /// ningún otro test lo notara.
    #[test]
    fn id_prefix_matches_the_declared_group() {
        for f in CATALOG {
            let expected_prefix = match f.group {
                FailureGroup::Elec => "elec.",
                FailureGroup::Hyd => "hyd.",
                FailureGroup::Apu => "apu.",
            };
            assert!(
                f.id.starts_with(expected_prefix),
                "'{}' declara grupo {:?} pero su id no empieza por '{expected_prefix}'",
                f.id,
                f.group
            );
        }
    }

    #[test]
    fn catalog_covers_the_phase4_hydraulic_failures() {
        // Las 13 entradas ATA29 de FBW (issue #55), ni una más ni una menos.
        let hyd: Vec<_> = CATALOG
            .iter()
            .filter(|f| f.group == FailureGroup::Hyd)
            .collect();
        assert_eq!(hyd.len(), 13, "se esperaban los 13 fallos ATA29 de FBW");
        for id in [
            "hyd.reservoir_leak.yellow",
            "hyd.reservoir_air_leak.green",
            "hyd.reservoir_return_leak.blue",
            "hyd.eng_pump_overheat.green",
            "hyd.elec_pump_overheat.yellow",
        ] {
            assert!(by_id(id).is_some(), "falta el fallo '{id}'");
        }
    }
}
