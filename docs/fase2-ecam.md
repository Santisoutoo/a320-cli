# Fase 2 — Nota de diseño: `read_ecam()` sin FWC

*(Escrita al abrir el issue #15, que exigía "investigación hecha y escrita" antes de estimar. El hallazgo cambia el alcance del issue de forma material, así que se registra también como **D-014** en `docs/decisiones.md`.)*

## La pregunta

`CLAUDE.md` anticipaba que `read_ecam()` sería "mapear los warnings del FWC (probablemente LVARs de warning a mensajes)". El issue #15 marcaba como riesgo abierto que el texto viviera en la capa TypeScript del ECAM y, en ese caso, que el catálogo de mensajes fuera nuestro. Había que resolverlo antes de escribir código.

## El hallazgo: no hay FWC en el Rust

**No existe FWC/FWS en el código vendorizado.** Búsqueda exhaustiva de `flight_warning`, `FlightWarningComputer`, `struct Fwc`, `struct Fws`, `master_caution`, `master_warning` en todo el árbol (`fbw-a32nx`, `fbw-a380x`, `fbw-common`): **cero coincidencias**. No hay módulo `fwc`/`fws` en `a320_systems/src/` ni en `systems/src/`.

No es que no lo hayamos encontrado: el propio vendor lo dice. En `fbw-a32nx/src/wasm/systems/a320_systems/src/surveillance.rs:73`:

```rust
self.discrete_inputs.audio_inhibit = false; // TODO: Comes from FWC during e.g. STALL STALL
```

El Rust trata al FWC como una fuente **externa aún no modelada**.

**El ECAM en TypeScript ni siquiera está vendorizado.** El submódulo está en sparse-checkout:

```
$ git -C core-rs/vendor/aircraft sparse-checkout list
fbw-a32nx/src/wasm
fbw-a380x/src/wasm
fbw-common/src/wasm
```

`fbw-a32nx/src/systems` (instrumentos, EWD/ECAM, MCDU) **no existe localmente**. Tampoco la raíz `docs/` del repo de FBW, así que no hay lista de simvars de warning que consultar.

**No hay master caution ni master warning en Rust.** Lo único cercano es `ECP_DISCRETE_OUT_EMER_CANC` (el pulsador EMER CANC del panel ECP), que es una *entrada* al TAWS, no una salida de un FWC. El TAWS/EGPWS (`fbw-common/.../systems/src/surveillance/`) es alertas de terreno, no el FWC del ECAM.

## Consecuencia

`read_ecam()` **no puede** ser "mapear los warnings del FWC": no hay FWC que mapear. El catálogo de mensajes ECAM es **nuestro**, y `read_ecam()` es un **motor de reglas** sobre variables que el Rust sí escribe.

Esto no es un atajo por comodidad, es la única opción disponible: portar el FWC entero es un subproyecto (y su lógica de inhibición por fase de vuelo es justamente lo que no está en el Rust), y el texto de los mensajes está en una capa que ni compilamos ni tenemos descargada.

## Lo que el Rust sí da

### Los cuatro faults del panel eléctrico

Son todos los `set_fault` de `fbw-a32nx/src/wasm/systems/a320_systems/src/electrical/mod.rs`:

| LVAR | Condición | Línea | ¿Alcanzable en Fase 2? |
|---|---|---|---|
| `OVHD_ELEC_APU_GEN_PB_HAS_FAULT` | `apu_gen.is_on() && apu.is_available() && apu_gen_contactor_open() && !ext_pwr_contactor_closed() && !both_engine_gen_contactors_closed()` | `:306` | **Sí** — gateado por el APU realmente disponible |
| `OVHD_ELEC_ENG_GEN_{1,2}_PB_HAS_FAULT` | `gen_contactor_open(n) && gen.is_on()` | `:303` | Sí, pero sin motores se enciende con solo poner el PB en ON |
| `OVHD_ELEC_AC_ESS_FEED_PB_HAS_FAULT` | `!ac_ess_bus_is_powered(electricity)` | `:297` | Sí, pero **incondicional** (ver trampa abajo) |
| `OVHD_EMER_ELEC_RAT_AND_EMER_GEN_PB_HAS_FAULT` | `in_emergency_elec() && !emer_gen_contactor_closed() && !context.is_on_ground()` | `:408` | **No** — exige `!is_on_ground()` |

Mecánica (`fbw-common/.../systems/src/overhead/mod.rs:64-70`): `OnOffFaultPushButton::write` escribe `OVHD_{name}_PB_IS_ON` y `OVHD_{name}_PB_HAS_FAULT` **cada tick**. Son LVARs reales del store, legibles con `peek_by_name`.

**Las baterías nunca reciben `set_fault`**: `OVHD_ELEC_BAT_{1,2}_PB_HAS_FAULT` es siempre `false`. El BAT FAULT no está modelado.

**Los TR no tienen luz de fault** — coherente con el avión real (el TR no tiene pulsador con FAULT en el overhead; se ve en la página ELEC del SD, no implementada). Lo que sí exponen es `ELEC_TR_{n}_POTENTIAL_NORMAL`.

### La trampa: la ECAM sucia en cold & dark

`ac_ess_feed.set_fault(!ac_ess_bus_is_powered(electricity))` **no está gateado por nada más**. En cold & dark el AC ESS no está alimentado, luego el flag es `true` **sin ningún fallo inyectado**. Lo confirma el propio test de FBW (`electrical/mod.rs:1721`, `when_ac_ess_bus_is_unpowered_ac_ess_feed_has_fault`).

Un mapeo naive flag→mensaje daría una caution en un avión perfectamente sano, violando el criterio de #15 ("ECAM limpia en cold & dark settled").

**No es un bug de FBW.** En el avión real esa lógica de inhibición vive en el FWC — que es justo lo que no está portado. Y en el avión real la ECAM ni siquiera está alimentada en cold & dark: no hay nada que mostrar.

**Nuestro fix**: un **gate de alimentación**. Las reglas solo se evalúan si la ECAM estaría viva; si no, `read_ecam()` devuelve lista vacía. Cumple el criterio del issue *y* es fiel al avión, que es la razón de verdad para hacerlo así.

(Nota: el mismo razonamiento se aplicaría a `ENG_GEN_{1,2}_PB_HAS_FAULT`, cuyos pulsadores arrancan en ON en FBW — pero **nuestro runtime no siembra** (D-007), así que esos PB leen 0 = OFF y no dan fault en cold & dark. La trampa nos afecta solo vía AC ESS FEED, que no depende de ningún pulsador.)

## El diseño: `EcamSource`

Cada regla declara **de quién es la lógica**:

- **`VendorFlag`** — el flag lo calcula FBW (`OVHD_*_PB_HAS_FAULT`). Alta confianza: es el modelo del avión de FBW diciendo que hay un fault.
- **`Derived`** — la regla es nuestra, sobre estado que FBW expone (p. ej. "TR alimentado pero sin potencial normal" → TR FAULT).

**Esto no es cosmético.** Es la frontera entre el ground truth que heredamos de FBW y el que inventamos nosotros. La contribución de investigación del proyecto es el entorno evaluable; si en la Fase 5 no podemos decir qué parte del ground truth es de FBW y qué parte nuestra, no podemos decir qué está midiendo el benchmark. Se registra por regla, no en un comentario.

En el mismo espíritu, el catálogo de Fase 2 **solo incluye lo alcanzable**: el RAT/EMER GEN queda fuera y documentado (exige `!is_on_ground()`, y la Fase 2 es toda en tierra). No se stubbea lo que no se puede levantar ni verificar.

## Lo que esto implica para la Fase 5

El catálogo de mensajes crecerá regla a regla, y cada una debe poder levantarse en un test. La honestidad del scoring depende de que `read_ecam()` no prometa más de lo que el modelo sabe: un mensaje que nunca se enciende, o que se enciende sin causa, contamina cada trayectoria que lo toque.

El campo `ata` del catálogo de fallos (D-013) y el `source` de aquí son las dos costuras que mantienen esto auditable cuando el pin del vendor se mueva.
