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
//! **Alcance de Fase 2: ATA24 (eléctrico)** — el único sistema que la Fase 1 sabe
//! observar. Los grupos de hidráulico/neumático/fuel/tren se añaden en su fase,
//! cuando exista cómo verificar que el fallo hace algo observable; catalogar
//! ahora un id que ningún test puede ejercitar es catalogar un id que puede
//! estar mal mapeado sin que nadie se entere.
//!
//! **Los ids ATA vienen de FBW.** La tabla `(u32, FailureType)` de
//! `a320_systems_wasm/src/lib.rs:101-163` es la numeración que usa el propio
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
use systems::shared::ElectricalBusType;

/// Sistema al que pertenece el fallo (para agrupar en CLI/MCP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureGroup {
    /// Sistema eléctrico (ATA24).
    Elec,
    // Hyd, Fuel, Engines, Brakes, RA... se añaden en fases posteriores.
}

impl FailureGroup {
    pub fn as_str(&self) -> &'static str {
        match self {
            FailureGroup::Elec => "ELEC",
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

/// Catálogo curado. **Fase 2: ATA24 (eléctrico).**
///
/// Correspondencia 1:1 con la tabla ATA24 de FBW
/// (`a320_systems_wasm/src/lib.rs:101-163`), 20 entradas.
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
    fn ata_ids_are_unique_and_in_the_ata24_range() {
        for (i, a) in CATALOG.iter().enumerate() {
            assert!(
                (24_000..25_000).contains(&a.ata),
                "'{}' tiene un ATA fuera del rango eléctrico: {}",
                a.id,
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

    #[test]
    fn all_phase2_failures_are_electrical() {
        assert!(CATALOG.iter().all(|f| f.group == FailureGroup::Elec));
    }
}
