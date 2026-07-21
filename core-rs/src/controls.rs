//! Catálogo **curado a mano** de controles del A320.
//!
//! `list_variables()` (registro) vuelca cientos de nombres crudos de LVAR: es
//! una herramienta de depuración, no una interfaz. Este catálogo es la otra
//! mitad — las cosas que un piloto puede *accionar* de verdad, con un nombre
//! amigable que un humano adivina y un LLM recibe, más los metadatos que un
//! esquema necesita (tipo, valores válidos, descripción, grupo). Lo consumen
//! los dos frontends: la CLI para autocompletar, el MCP para el esquema de
//! `set_control`.
//!
//! **Curado, no generado**: el objetivo es que *una persona* haya decidido que
//! estos son los controles reales. Un volcado del registro obligaría al LLM a
//! adivinar cuáles de cientos de variables son switches escribibles.
//!
//! **Alcance de Fase 1**: el panel eléctrico (baterías, ext pwr, APU gen, bus
//! tie, generadores). Se amplía por fase.
//!
//! **Cabina vs mundo**: `OVHD_ELEC_BAT_1_PB_IS_AUTO` es un pulsador de cabina;
//! `EXT_PWR_AVAIL:1` es estado de "mundo" que en un sim real vendría de MSFS y
//! aquí falsificamos (si hay un GPU enchufado). Ambos los escribe el frontend,
//! solo uno es un control de cabina: se distinguen con [`ControlDomain`].

/// Tipo de dato de un control (issue #10: bool/enum/float).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    /// Booleano (pulsador/switch de dos estados).
    Bool,
    /// Selector discreto de un conjunto de posiciones.
    Enum,
    /// Magnitud continua (knob).
    Float,
}

impl ControlKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ControlKind::Bool => "bool",
            ControlKind::Enum => "enum",
            ControlKind::Float => "float",
        }
    }
}

/// Dominio del control: cabina real vs mundo exterior falsificado.
///
/// La distinción importa para el frontend: un control de cabina es algo que el
/// piloto/agente acciona como parte del procedimiento; un fake de mundo es un
/// estado del entorno que en un sim real no tocaría (lo pondría MSFS), y que
/// aquí exponemos solo para poder montar escenarios (p. ej. "hay GPU").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlDomain {
    /// Pulsador/knob real de cabina que un piloto acciona.
    Cockpit,
    /// Estado de "mundo" que falsificamos (lo daría MSFS en un sim real).
    World,
}

impl ControlDomain {
    pub fn as_str(&self) -> &'static str {
        match self {
            ControlDomain::Cockpit => "cockpit",
            ControlDomain::World => "world",
        }
    }
}

/// Sistema al que pertenece el control (para agrupar en CLI/MCP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlGroup {
    /// Sistema eléctrico.
    Elec,
    /// Sistema hidráulico.
    Hyd,
    /// Unidad de potencia auxiliar.
    Apu,
    /// Sistema de combustible.
    Fuel,
    /// Motores (panel ENG: masters + selector de modo).
    Eng,
    // Pneu... se añaden en fases posteriores.
}

impl ControlGroup {
    pub fn as_str(&self) -> &'static str {
        match self {
            ControlGroup::Elec => "ELEC",
            ControlGroup::Hyd => "HYD",
            ControlGroup::Apu => "APU",
            ControlGroup::Fuel => "FUEL",
            ControlGroup::Eng => "ENG",
        }
    }
}

/// Valores admisibles de un control.
///
/// Es la fuente de verdad de la validación de [`crate::api::Sim::set`]: un valor
/// que no pase [`ValidValues::check`] se rechaza con un error accionable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValidValues {
    /// Booleano: solo `0.0` (off/false) o `1.0` (on/true).
    Bool,
    /// Conjunto discreto de valores válidos (para selectores).
    Enum(&'static [f64]),
    /// Rango continuo inclusivo `[min, max]`.
    Range { min: f64, max: f64 },
}

impl ValidValues {
    /// ¿Es `value` admisible? Devuelve `Ok(())` o un motivo legible para el
    /// error (el REPL y el LLM necesitan saber *por qué* se rechazó).
    pub fn check(&self, value: f64) -> Result<(), String> {
        match self {
            ValidValues::Bool => {
                if value == 0.0 || value == 1.0 {
                    Ok(())
                } else {
                    Err("must be 0 (off) or 1 (on)".to_owned())
                }
            }
            ValidValues::Enum(allowed) => {
                if allowed.contains(&value) {
                    Ok(())
                } else {
                    Err(format!("must be one of {allowed:?}"))
                }
            }
            ValidValues::Range { min, max } => {
                if value >= *min && value <= *max {
                    Ok(())
                } else {
                    Err(format!("must be within [{min}, {max}]"))
                }
            }
        }
    }

    /// Descripción legible de los valores válidos (para esquemas y ayuda).
    pub fn describe(&self) -> String {
        match self {
            ValidValues::Bool => "0 (off) or 1 (on)".to_owned(),
            ValidValues::Enum(allowed) => format!("one of {allowed:?}"),
            ValidValues::Range { min, max } => format!("[{min}, {max}]"),
        }
    }
}

/// Una entrada del catálogo: un control accionable con sus metadatos.
///
/// `Copy` porque todos los campos son `Copy` (`&'static str` + enums): el
/// catálogo es estático y las entradas se pasan por valor sin coste.
#[derive(Debug, Clone, Copy)]
pub struct Control {
    /// Nombre amigable y estable (lo que se teclea / se le da al LLM).
    pub name: &'static str,
    /// LVAR subyacente que se escribe realmente en el registro.
    pub lvar: &'static str,
    /// Tipo de dato (bool/enum/float).
    pub kind: ControlKind,
    /// Valores admisibles.
    pub valid: ValidValues,
    /// Descripción de una línea.
    pub description: &'static str,
    /// Grupo por sistema.
    pub group: ControlGroup,
    /// Cabina real vs fake de mundo.
    pub domain: ControlDomain,
}

/// Catálogo curado. **Fase 1: panel eléctrico. Fase 4: panel hidráulico
/// (slice 1), panel APU (slice 2), fuel como mundo (slice 3) y panel ENG
/// (slice 4).**
///
/// Los LVAR provienen del panel eléctrico superior de FBW
/// (`a320_systems/src/electrical/mod.rs`, `A320ElectricalOverheadPanel::new`) y
/// del `ExternalPowerSource` (`systems/.../external_power_source.rs`). Los
/// sufijos `_PB_IS_AUTO` / `_PB_IS_ON` los fija el tipo de pulsador de FBW:
/// `AutoOffFaultPushButton` usa AUTO, `OnOffFaultPushButton`/`OnOffAvailable`
/// usan ON. El test `every_catalog_lvar_is_registered_after_a_tick` verifica
/// que cada `lvar` de aquí existe en el registro (caza typos y drift del vendor).
///
/// El panel hidráulico (`a320_systems/src/hydraulic/mod.rs`,
/// `A320HydraulicOverheadPanel::new`, líneas 4484-4520) añade dos tipos más con
/// semántica propia:
///
/// - `AutoOnFaultPushButton` (bomba eléctrica amarilla): el LVAR sigue siendo
///   `_PB_IS_AUTO`, pero las posiciones son AUTO/ON — **0 = ON (la bomba
///   funciona), 1 = AUTO (parada en tierra salvo operación de cargo door)**. Es
///   el inverso del patrón AUTO/OFF del resto del panel.
/// - `MomentaryPushButton` / `MomentaryOnPushButton` (RAT man on, blue pump
///   ovrd): el LVAR de entrada es `_IS_PRESSED` (sin `PB`). El ovrd además
///   conmuta su estado interno en cada flanco de subida de `_IS_PRESSED` (el
///   estado resultante se lee en `OVHD_HYD_EPUMPY_OVRD_IS_ON`, que escribe FBW).
///
/// **Gotcha del vendor**: el pulsador `HYD_EPUMPY_OVRD` es, pese al nombre del
/// LVAR, el **BLUE PUMP OVRD** del panel de mantenimiento — lo consume
/// exclusivamente `A320BlueElectricPumpController` (hydraulic/mod.rs:3139) y el
/// panel lo apaga si la bomba azul está en OFF (`update_blue_override_state`,
/// :4523-4527). El nombre amigable (`hyd_epump_blue_ovrd`) sigue la semántica
/// real, no el nombre del LVAR.
///
/// El panel APU (Fase 4, slice 2) son dos pulsadores del overhead APU más el
/// APU BLEED del panel neumático:
///
/// - MASTER SW y START se construyen en el propio módulo APU de `systems`
///   (`fbw-common/.../systems/src/apu/mod.rs:352-353`:
///   `OnOffFaultPushButton::new_off(context, "APU_MASTER_SW")` y
///   `OnOffAvailablePushButton::new_off(context, "APU_START")`), de donde salen
///   `OVHD_APU_MASTER_SW_PB_IS_ON` y `OVHD_APU_START_PB_IS_ON`.
/// - **Gotcha del bleed**: el LVAR real es `OVHD_PNEU_APU_BLEED_PB_IS_ON` — el
///   pulsador vive en el panel neumático y se construye con el nombre
///   `"PNEU_APU_BLEED"` (`a320_systems/src/pneumatic.rs:1545`:
///   `OnOffFaultPushButton::new_on(context, "PNEU_APU_BLEED")`; el prefijo
///   `OVHD_` y el sufijo `_PB_IS_ON` los añade `overhead/mod.rs:24-25`). El
///   nombre "obvio" `OVHD_APU_BLEED_PB_IS_ON` solo existe en un helper del test
///   bed del vendor (pneumatic.rs:2706-2707) y **ningún sistema lo lee**:
///   apuntar ahí sería escribir una variable muerta.
///
/// El grupo Fuel (Fase 4, slice 3) es **dominio World entero**: el Rust de FBW
/// no modela consumo ni crossfeed — los tanques son simvars de *entrada* en
/// galones US que en un sim real escribiría MSFS y aquí falsificamos (el
/// runtime los siembra una vez, ver `runtime::FUEL_SEED_GALLONS`):
///
/// - Tanques y capacidades: `A320_FUEL` (`a320_systems/src/fuel/mod.rs:53-79`);
///   center 2179 gal, mains 1816 gal, aux 228 gal. El wording MSFS es
///   `FUEL TANK LEFT MAIN QUANTITY` (no "INNER"): en el mapeo de FBW
///   `LeftInner` = LEFT MAIN y `LeftOuter` = LEFT AUX
///   (`A320FuelTankType`, `fuel/mod.rs:23-47`).
/// - `UNLIMITED FUEL` (`fbw-common/.../fuel/mod.rs:133`): con 1, todo
///   `tank_has_fuel` responde `true` ignorando las cantidades (`:148-150`).
/// - Bombas `FUELSYSTEM PUMP ACTIVE:{id}` (`fbw-common/.../fuel/mod.rs:212`;
///   ids 2/5 left, 3/6 right, 7 APU, `a320_systems/src/fuel/mod.rs:82-123`):
///   en el Rust del vendor solo alimentan el **consumo eléctrico** de la bomba
///   — no mueven combustible ni condicionan al APU.
pub const CATALOG: &[Control] = &[
    Control {
        name: "bat_1",
        lvar: "OVHD_ELEC_BAT_1_PB_IS_AUTO",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Battery 1 master pushbutton: 1 = AUTO (battery in the loop), 0 = OFF",
        group: ControlGroup::Elec,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "bat_2",
        lvar: "OVHD_ELEC_BAT_2_PB_IS_AUTO",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Battery 2 master pushbutton: 1 = AUTO (battery in the loop), 0 = OFF",
        group: ControlGroup::Elec,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "ext_pwr",
        lvar: "OVHD_ELEC_EXT_PWR_PB_IS_ON",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "External power pushbutton: 1 = ON (GPU feeds the AC network), 0 = OFF",
        group: ControlGroup::Elec,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "apu_gen",
        lvar: "OVHD_ELEC_APU_GEN_PB_IS_ON",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "APU generator pushbutton: 1 = ON (APU gen supplies the network), 0 = OFF",
        group: ControlGroup::Elec,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "bus_tie",
        lvar: "OVHD_ELEC_BUS_TIE_PB_IS_AUTO",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Bus tie pushbutton: 1 = AUTO (buses tie as needed), 0 = OFF (buses isolated)",
        group: ControlGroup::Elec,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "gen_1",
        lvar: "OVHD_ELEC_ENG_GEN_1_PB_IS_ON",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Engine 1 generator pushbutton: 1 = ON, 0 = OFF",
        group: ControlGroup::Elec,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "gen_2",
        lvar: "OVHD_ELEC_ENG_GEN_2_PB_IS_ON",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Engine 2 generator pushbutton: 1 = ON, 0 = OFF",
        group: ControlGroup::Elec,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "ext_pwr_avail",
        lvar: "EXT_PWR_AVAIL:1",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "External power availability (world state we fake): 1 = GPU plugged in, 0 = not connected",
        group: ControlGroup::Elec,
        domain: ControlDomain::World,
    },
    // --- Panel hidráulico (Fase 4, slice 1) ---------------------------------
    Control {
        name: "hyd_eng_1_pump",
        lvar: "OVHD_HYD_ENG_1_PUMP_PB_IS_AUTO",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Engine 1 (green) hydraulic pump pushbutton: 1 = AUTO (pump pressurises when engine 1 runs), 0 = OFF",
        group: ControlGroup::Hyd,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "hyd_eng_2_pump",
        lvar: "OVHD_HYD_ENG_2_PUMP_PB_IS_AUTO",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Engine 2 (yellow) hydraulic pump pushbutton: 1 = AUTO (pump pressurises when engine 2 runs), 0 = OFF",
        group: ControlGroup::Hyd,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "hyd_epump_blue",
        lvar: "OVHD_HYD_EPUMPB_PB_IS_AUTO",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Blue electric pump pushbutton: 1 = AUTO (runs airborne, with an engine running, or with the blue pump override), 0 = OFF",
        group: ControlGroup::Hyd,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "hyd_epump_yellow",
        lvar: "OVHD_HYD_EPUMPY_PB_IS_AUTO",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Yellow electric pump pushbutton (AUTO/ON, inverted!): 1 = AUTO (pump stopped on ground unless cargo door operation), 0 = ON (pump runs while AC powered)",
        group: ControlGroup::Hyd,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "hyd_epump_blue_ovrd",
        lvar: "OVHD_HYD_EPUMPY_OVRD_IS_PRESSED",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Blue pump override momentary pushbutton (vendor LVAR says EPUMPY but it overrides the BLUE pump): each 0->1 press toggles the override; requires hyd_epump_blue in AUTO",
        group: ControlGroup::Hyd,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "hyd_ptu",
        lvar: "OVHD_HYD_PTU_PB_IS_AUTO",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Power transfer unit pushbutton: 1 = AUTO (PTU transfers pressure between green and yellow when enabled), 0 = OFF",
        group: ControlGroup::Hyd,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "hyd_rat_man_on",
        lvar: "OVHD_HYD_RAT_MAN_ON_IS_PRESSED",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "RAT manual deploy momentary pushbutton: 1 = pressed (deploys the ram air turbine if its solenoid is powered), 0 = released",
        group: ControlGroup::Hyd,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "hyd_leak_measurement_g",
        lvar: "OVHD_HYD_LEAK_MEASUREMENT_G_PB_IS_AUTO",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Green leak measurement valve pushbutton (maintenance panel): 1 = AUTO (valve open, normal ops), 0 = OFF (valve closed)",
        group: ControlGroup::Hyd,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "hyd_leak_measurement_b",
        lvar: "OVHD_HYD_LEAK_MEASUREMENT_B_PB_IS_AUTO",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Blue leak measurement valve pushbutton (maintenance panel): 1 = AUTO (valve open, normal ops), 0 = OFF (valve closed)",
        group: ControlGroup::Hyd,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "hyd_leak_measurement_y",
        lvar: "OVHD_HYD_LEAK_MEASUREMENT_Y_PB_IS_AUTO",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Yellow leak measurement valve pushbutton (maintenance panel): 1 = AUTO (valve open, normal ops), 0 = OFF (valve closed)",
        group: ControlGroup::Hyd,
        domain: ControlDomain::Cockpit,
    },
    // --- Panel APU (Fase 4, slice 2) ----------------------------------------
    Control {
        name: "apu_master",
        lvar: "OVHD_APU_MASTER_SW_PB_IS_ON",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "APU master switch pushbutton: 1 = ON (powers the ECB and opens the air intake flap), 0 = OFF (orderly shutdown)",
        group: ControlGroup::Apu,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "apu_start",
        lvar: "OVHD_APU_START_PB_IS_ON",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "APU start pushbutton: 1 = ON (starts the APU once the master switch is on), 0 = released",
        group: ControlGroup::Apu,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "apu_bleed",
        lvar: "OVHD_PNEU_APU_BLEED_PB_IS_ON",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "APU bleed pushbutton (pneumatic panel): 1 = ON (bleed valve opens once the APU runs above 95% N), 0 = OFF",
        group: ControlGroup::Apu,
        domain: ControlDomain::Cockpit,
    },
    // --- Panel ENG (Fase 4, slice 4) -----------------------------------------
    //
    // Los LVARs `ENG_MASTER_{1,2}` son NUESTROS, no del vendor: en MSFS el
    // engine master vive en el fuel system C++ y ningún elemento del Rust del
    // vendor lo registra — los lee nuestro `EngineModel` (`src/engine.rs`) y
    // los siembra el runtime (`ENGINE_CONTROL_SEED`). Ver D-020. El selector de
    // modo sí es del vendor: `TURB ENG IGNITION SWITCH EX1:1`, un único
    // selector para ambos motores, leído por el FADEC de pneumatic
    // (`a320_systems/src/pneumatic.rs:1608-1609`) con el enum
    // `EngineModeSelector` 0=CRANK / 1=NORM / 2=IGN-START
    // (`fbw-common/.../pneumatic/mod.rs:764-782`).
    Control {
        name: "eng_master_1",
        lvar: "ENG_MASTER_1",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Engine 1 master lever (our LVAR, not the vendor's): 1 = ON (starts with mode selector at IGN/START; keeps the engine running), 0 = OFF (shuts the engine down)",
        group: ControlGroup::Eng,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "eng_master_2",
        lvar: "ENG_MASTER_2",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Engine 2 master lever (our LVAR, not the vendor's): 1 = ON (starts with mode selector at IGN/START; keeps the engine running), 0 = OFF (shuts the engine down)",
        group: ControlGroup::Eng,
        domain: ControlDomain::Cockpit,
    },
    Control {
        name: "eng_mode",
        lvar: "TURB ENG IGNITION SWITCH EX1:1",
        kind: ControlKind::Enum,
        valid: ValidValues::Enum(&[0.0, 1.0, 2.0]),
        description: "Engine mode selector, one rotary selector for both engines: 0 = CRANK, 1 = NORM (rest position), 2 = IGN/START (arms the start when a master goes ON)",
        group: ControlGroup::Eng,
        domain: ControlDomain::Cockpit,
    },
    // --- Fuel como estado de mundo (Fase 4, slice 3) -------------------------
    Control {
        name: "fuel_tank_center",
        lvar: "FUEL TANK CENTER QUANTITY",
        kind: ControlKind::Float,
        valid: ValidValues::Range {
            min: 0.0,
            max: 2179.0,
        },
        description: "Center tank quantity in US gallons (world state we fake; the vendor Rust reads tank quantities but never burns fuel). Seeded empty by default",
        group: ControlGroup::Fuel,
        domain: ControlDomain::World,
    },
    Control {
        name: "fuel_tank_left_main",
        lvar: "FUEL TANK LEFT MAIN QUANTITY",
        kind: ControlKind::Float,
        valid: ValidValues::Range {
            min: 0.0,
            max: 1816.0,
        },
        description: "Left main (inner) tank quantity in US gallons (world state we fake). The APU feeds from this tank: draining it while the APU runs shuts it down",
        group: ControlGroup::Fuel,
        domain: ControlDomain::World,
    },
    Control {
        name: "fuel_tank_left_aux",
        lvar: "FUEL TANK LEFT AUX QUANTITY",
        kind: ControlKind::Float,
        valid: ValidValues::Range {
            min: 0.0,
            max: 228.0,
        },
        description: "Left aux (outer) tank quantity in US gallons (world state we fake)",
        group: ControlGroup::Fuel,
        domain: ControlDomain::World,
    },
    Control {
        name: "fuel_tank_right_main",
        lvar: "FUEL TANK RIGHT MAIN QUANTITY",
        kind: ControlKind::Float,
        valid: ValidValues::Range {
            min: 0.0,
            max: 1816.0,
        },
        description: "Right main (inner) tank quantity in US gallons (world state we fake)",
        group: ControlGroup::Fuel,
        domain: ControlDomain::World,
    },
    Control {
        name: "fuel_tank_right_aux",
        lvar: "FUEL TANK RIGHT AUX QUANTITY",
        kind: ControlKind::Float,
        valid: ValidValues::Range {
            min: 0.0,
            max: 228.0,
        },
        description: "Right aux (outer) tank quantity in US gallons (world state we fake)",
        group: ControlGroup::Fuel,
        domain: ControlDomain::World,
    },
    Control {
        name: "unlimited_fuel",
        lvar: "UNLIMITED FUEL",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Unlimited fuel flag (world state we fake): 1 = every tank reads as having fuel regardless of quantity, 0 = tank quantities rule",
        group: ControlGroup::Fuel,
        domain: ControlDomain::World,
    },
    Control {
        name: "fuel_pump_left_1",
        lvar: "FUELSYSTEM PUMP ACTIVE:2",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Left main tank pump 1 active flag (world state we fake): in the vendor Rust it only drives the pump's electrical consumption, it moves no fuel",
        group: ControlGroup::Fuel,
        domain: ControlDomain::World,
    },
    Control {
        name: "fuel_pump_left_2",
        lvar: "FUELSYSTEM PUMP ACTIVE:5",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Left main tank pump 2 active flag (world state we fake): in the vendor Rust it only drives the pump's electrical consumption, it moves no fuel",
        group: ControlGroup::Fuel,
        domain: ControlDomain::World,
    },
    Control {
        name: "fuel_pump_right_1",
        lvar: "FUELSYSTEM PUMP ACTIVE:3",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Right main tank pump 1 active flag (world state we fake): in the vendor Rust it only drives the pump's electrical consumption, it moves no fuel",
        group: ControlGroup::Fuel,
        domain: ControlDomain::World,
    },
    Control {
        name: "fuel_pump_right_2",
        lvar: "FUELSYSTEM PUMP ACTIVE:6",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "Right main tank pump 2 active flag (world state we fake): in the vendor Rust it only drives the pump's electrical consumption, it moves no fuel",
        group: ControlGroup::Fuel,
        domain: ControlDomain::World,
    },
    Control {
        name: "fuel_pump_apu",
        lvar: "FUELSYSTEM PUMP ACTIVE:7",
        kind: ControlKind::Bool,
        valid: ValidValues::Bool,
        description: "APU fuel pump active flag (world state we fake): in the vendor Rust it only drives the pump's electrical consumption; the APU itself only checks that the left main tank is not empty",
        group: ControlGroup::Fuel,
        domain: ControlDomain::World,
    },
];

/// Busca una entrada por su nombre amigable.
pub fn by_name(name: &str) -> Option<&'static Control> {
    CATALOG.iter().find(|c| c.name == name)
}

/// Busca una entrada por su LVAR subyacente.
pub fn by_lvar(lvar: &str) -> Option<&'static Control> {
    CATALOG.iter().find(|c| c.lvar == lvar)
}

/// Resuelve un identificador de control (nombre amigable **o** LVAR) a su
/// entrada del catálogo. Aceptar el LVAR además del nombre amigable mantiene
/// compatible el camino de escritura por LVAR crudo de la Fase 1 (issue #9).
pub fn resolve(control: &str) -> Option<&'static Control> {
    by_name(control).or_else(|| by_lvar(control))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_the_phase1_electrical_panel() {
        // Los controles que el issue #10 exige para Fase 1.
        for name in [
            "bat_1", "bat_2", "ext_pwr", "apu_gen", "bus_tie", "gen_1", "gen_2",
        ] {
            assert!(by_name(name).is_some(), "falta el control '{name}'");
        }
    }

    #[test]
    fn catalog_covers_the_phase4_hydraulic_panel() {
        // Los controles que el issue #55 (slice 1) exige para el panel Hyd.
        for name in [
            "hyd_eng_1_pump",
            "hyd_eng_2_pump",
            "hyd_epump_blue",
            "hyd_epump_yellow",
            "hyd_epump_blue_ovrd",
            "hyd_ptu",
            "hyd_rat_man_on",
            "hyd_leak_measurement_g",
            "hyd_leak_measurement_b",
            "hyd_leak_measurement_y",
        ] {
            let c = by_name(name).unwrap_or_else(|| panic!("falta el control '{name}'"));
            assert_eq!(c.group, ControlGroup::Hyd, "'{name}' debería ser HYD");
            assert_eq!(c.domain, ControlDomain::Cockpit, "'{name}' es de cabina");
        }
    }

    #[test]
    fn catalog_covers_the_phase4_apu_panel() {
        // Los controles que el issue #56 (slice 2) exige para el panel APU.
        for name in ["apu_master", "apu_start", "apu_bleed"] {
            let c = by_name(name).unwrap_or_else(|| panic!("falta el control '{name}'"));
            assert_eq!(c.group, ControlGroup::Apu, "'{name}' debería ser APU");
            assert_eq!(c.domain, ControlDomain::Cockpit, "'{name}' es de cabina");
        }
    }

    #[test]
    fn catalog_covers_the_phase4_engine_panel() {
        // Los controles que el issue #58 (slice 4) exige para el panel ENG.
        for name in ["eng_master_1", "eng_master_2", "eng_mode"] {
            let c = by_name(name).unwrap_or_else(|| panic!("falta el control '{name}'"));
            assert_eq!(c.group, ControlGroup::Eng, "'{name}' debería ser ENG");
            assert_eq!(c.domain, ControlDomain::Cockpit, "'{name}' es de cabina");
        }
        // El selector real del vendor (un solo selector para ambos motores) y
        // sus tres posiciones exactas: CRANK=0 / NORM=1 / IGN-START=2.
        let mode = by_name("eng_mode").unwrap();
        assert_eq!(mode.lvar, "TURB ENG IGNITION SWITCH EX1:1");
        assert_eq!(mode.valid, ValidValues::Enum(&[0.0, 1.0, 2.0]));
    }

    #[test]
    fn apu_bleed_writes_the_pneu_prefixed_lvar_not_the_test_bed_decoy() {
        // El pulsador real se llama "PNEU_APU_BLEED" (pneumatic.rs:1545); el
        // nombre sin PNEU_ solo existe en el test bed del vendor y ningún
        // sistema lo lee (ver la doc del catálogo).
        assert_eq!(
            by_name("apu_bleed").unwrap().lvar,
            "OVHD_PNEU_APU_BLEED_PB_IS_ON"
        );
    }

    #[test]
    fn momentary_pushbuttons_write_is_pressed_not_pb_is_on() {
        // Los momentary del vendor usan `OVHD_*_IS_PRESSED` (overhead/mod.rs:614
        // y :666), sin el segmento `PB`. Un catálogo que apuntase a `_PB_IS_ON`
        // escribiría una variable que el avión jamás lee.
        assert_eq!(
            by_name("hyd_rat_man_on").unwrap().lvar,
            "OVHD_HYD_RAT_MAN_ON_IS_PRESSED"
        );
        assert_eq!(
            by_name("hyd_epump_blue_ovrd").unwrap().lvar,
            "OVHD_HYD_EPUMPY_OVRD_IS_PRESSED"
        );
    }

    #[test]
    fn friendly_names_are_unique() {
        for (i, a) in CATALOG.iter().enumerate() {
            for b in &CATALOG[i + 1..] {
                assert_ne!(a.name, b.name, "nombre amigable duplicado: {}", a.name);
            }
        }
    }

    #[test]
    fn lvars_are_unique() {
        for (i, a) in CATALOG.iter().enumerate() {
            for b in &CATALOG[i + 1..] {
                assert_ne!(a.lvar, b.lvar, "LVAR duplicado: {}", a.lvar);
            }
        }
    }

    #[test]
    fn cockpit_controls_are_distinguished_from_world_fakes() {
        // El pulsador de ext pwr es cabina; la disponibilidad de GPU es mundo.
        assert_eq!(by_name("ext_pwr").unwrap().domain, ControlDomain::Cockpit);
        assert_eq!(
            by_name("ext_pwr_avail").unwrap().domain,
            ControlDomain::World
        );
        // Los fakes de mundo son EXT_PWR_AVAIL:1 (Fase 1) y el grupo Fuel
        // entero (Fase 4, slice 3): el Rust de FBW no modela consumo, así que
        // tanques/bombas/unlimited son mundo, no cabina.
        let world: Vec<_> = CATALOG
            .iter()
            .filter(|c| c.domain == ControlDomain::World)
            .map(|c| c.name)
            .collect();
        assert_eq!(
            world,
            vec![
                "ext_pwr_avail",
                "fuel_tank_center",
                "fuel_tank_left_main",
                "fuel_tank_left_aux",
                "fuel_tank_right_main",
                "fuel_tank_right_aux",
                "unlimited_fuel",
                "fuel_pump_left_1",
                "fuel_pump_left_2",
                "fuel_pump_right_1",
                "fuel_pump_right_2",
                "fuel_pump_apu",
            ]
        );
    }

    #[test]
    fn catalog_covers_the_phase4_fuel_world_state() {
        // Los controles que el issue #57 (slice 3) exige para el dominio Fuel:
        // todos World (no hay pulsadores de fuel en el Rust del vendor).
        for name in [
            "fuel_tank_center",
            "fuel_tank_left_main",
            "fuel_tank_left_aux",
            "fuel_tank_right_main",
            "fuel_tank_right_aux",
            "unlimited_fuel",
            "fuel_pump_left_1",
            "fuel_pump_left_2",
            "fuel_pump_right_1",
            "fuel_pump_right_2",
            "fuel_pump_apu",
        ] {
            let c = by_name(name).unwrap_or_else(|| panic!("falta el control '{name}'"));
            assert_eq!(c.group, ControlGroup::Fuel, "'{name}' debería ser FUEL");
            assert_eq!(c.domain, ControlDomain::World, "'{name}' es fake de mundo");
        }
    }

    #[test]
    fn tank_ranges_match_the_vendor_capacities() {
        // Capacidades de `A320_FUEL` (a320_systems/src/fuel/mod.rs:53-79), en
        // galones US. Si upstream las cambiara, este test lo delata.
        for (name, capacity) in [
            ("fuel_tank_center", 2179.0),
            ("fuel_tank_left_main", 1816.0),
            ("fuel_tank_left_aux", 228.0),
            ("fuel_tank_right_main", 1816.0),
            ("fuel_tank_right_aux", 228.0),
        ] {
            assert_eq!(
                by_name(name).unwrap().valid,
                ValidValues::Range {
                    min: 0.0,
                    max: capacity
                },
                "rango de '{name}'"
            );
        }
    }

    #[test]
    fn resolve_accepts_both_friendly_name_and_lvar() {
        let by_friendly = resolve("bat_1").unwrap();
        let by_raw = resolve("OVHD_ELEC_BAT_1_PB_IS_AUTO").unwrap();
        assert_eq!(by_friendly.lvar, by_raw.lvar);
        assert_eq!(by_friendly.name, "bat_1");
        assert!(resolve("no_such_control").is_none());
    }

    #[test]
    fn bool_validation_accepts_only_zero_and_one() {
        let v = ValidValues::Bool;
        assert!(v.check(0.0).is_ok());
        assert!(v.check(1.0).is_ok());
        assert!(v.check(0.5).is_err());
        assert!(v.check(2.0).is_err());
        assert!(v.check(-1.0).is_err());
    }

    #[test]
    fn range_validation_is_inclusive() {
        let v = ValidValues::Range {
            min: 0.0,
            max: 10.0,
        };
        assert!(v.check(0.0).is_ok());
        assert!(v.check(10.0).is_ok());
        assert!(v.check(5.0).is_ok());
        assert!(v.check(-0.1).is_err());
        assert!(v.check(10.1).is_err());
    }

    #[test]
    fn enum_validation_matches_the_allowed_set() {
        let v = ValidValues::Enum(&[1.0, 2.0, 3.0]);
        assert!(v.check(2.0).is_ok());
        assert!(v.check(4.0).is_err());
    }
}
