# Registro de decisiones de arquitectura

Una entrada por decisión. Las decisiones que afectan a la reproducibilidad del benchmark (pin de FBW, parches al código vendorizado) son obligatorias de registrar.

## Tomadas

### D-001 — Base: A320 de FlyByWire (Zibo 737 descartado)
**Fecha**: 2026-07-15 (del brief inicial)
Los crates `systems` + `a320_systems` de FBW (Rust) no dependen de MSFS en runtime y su CI los corre headless. Zibo no es open-source, no tiene motor de sistemas propio (XLua dentro de X-Plane) y no corre headless.

### D-002 — FBW vendorizado y pineado
**Fecha**: 2026-07-15 (del brief inicial; pin fijado en Fase 0)
Submódulo o subtree con pin a commit/tag concreto. FBW actualiza casi cada semana; la reproducibilidad del benchmark exige el pin. Todo cambio de pin se registra aquí como nueva entrada.
**Pin actual**: `13bce4bcf5a1edce34671145290ce9db0455ea57` (rama `master`, 2026-07-14). Se pinea a commit y no a tag porque los tags upstream están abandonados desde 2024 (último: `v2024.1.0-rc5`). Toolchain asociado: Rust 1.93.0 (según `rust-toolchain.toml` del monorepo).

### D-005 — El decoupling de msfs-rs resultó innecesario
**Fecha**: 2026-07-15 (exploración de Fase 0)
El brief asumía que habría que stubbear dependencias de `msfs-rs` para compilar nativo. La exploración del monorepo pineado demuestra que no: `systems` (`fbw-common/src/wasm/systems/systems`) y `a320_systems` (`fbw-a32nx/src/wasm/systems/a320_systems`) **no declaran ninguna dependencia de `msfs`** en sus `Cargo.toml`. Todo el acoplamiento MSFS vive en `systems_wasm` y `a320_systems_wasm`, que quedan **fuera del grafo de dependencias** de los crates que usamos. El único `cfg(target_arch = "wasm32")` del código objetivo (`systems/src/shared/random.rs`) ya trae rama nativa (`not(wasm32)` con `rand` puro).
**Consecuencia**: el "decoupling" de la Fase 0 se reduce a *no compilar* los crates `*_wasm`. No hay stubs ni parches al código vendorizado.
**Único obstáculo real del spike**: con el vendor anidado bajo `core-rs/`, cargo resolvía la herencia `workspace = true` de los crates de FBW contra nuestro workspace en vez del suyo. Solución de una línea en `core-rs/Cargo.toml`: `[workspace] exclude = ["vendor"]`. Confirmado empíricamente: cero parches al vendor (tests eléctricos upstream: 102 passed en nativo).

### D-003 — Licencia GPLv3
**Fecha**: 2026-07-15 (del brief inicial)
Al vendorizar los crates de FBW, el proyecto hereda GPLv3. Aceptado (proyecto personal/open).

### D-004 — Stack: Rust + PyO3 (Rust-puro descartado)
**Fecha**: 2026-07-15 (decidido por el usuario al cerrar la Fase 0)
Core en Rust expuesto a Python vía **PyO3** (crate `bindings/`); CLI y servidor MCP en **Python** (SDK oficial de MCP). Motivo principal: la capa de benchmark/orquestación de agentes de la Fase 5 es mucho más cómoda en Python, y el spike demostró que el FFI es trivial (ver criterios abajo). Alternativa descartada: todo Rust con `rmcp`.
**Criterios que respaldaron la decisión**:
- La superficie a exponer es pequeña y estable (el contrato de la API: `set`/`get`/`step`/`read_ecam`/failures/`snapshot`/`list_*`), lo que abarata cualquiera de las dos opciones.
- Toda la interacción con el avión pasa por lectura/escritura de variables por nombre (`f64`/`bool`) más un enum de failures — tipos triviales de cruzar por FFI; PyO3 no tendría que exponer tipos complejos de FBW.
- El harness público de FBW (`SimulationTestBed`) y el camino `Simulation<A320>` directo son ambos Rust puro sin async, así que un wrapper PyO3 sería un objeto con métodos síncronos, el caso fácil.
- A favor de Rust-puro estaba: un solo toolchain y distribución de un único binario; pesó más la ergonomía de Python para la Fase 5.

### D-006 — Flujo de ramas: una issue → una rama → un PR a `dev`; `dev` → `main` integra
**Fecha**: 2026-07-15
`main` es la rama estable y **solo recibe código vía pull request**. El trabajo del día a día va en `dev`. Cada issue lleva **su propia rama y su propio PR contra `dev`**; `main` solo recibe PRs de integración desde `dev` al cerrar una fase — es decir, **una integración por epic**.

El PR no es ceremonia: es el único punto donde corre el CI y donde se lee la checklist (pin del vendor intacto, cero parches al vendor, build nativo, GPLv3). Son justo las cosas que, saltadas una vez por inercia, rompen en silencio la reproducibilidad del benchmark. Que el proyecto sea de una sola persona no cambia eso; si acaso lo empeora, porque no hay nadie más que lo note.

**Configuración efectiva en GitHub** (protección clásica sobre `main`):
- Pull request obligatorio; `required_approving_review_count: 0` — con un solo autor, exigir aprobación se auto-bloquearía (nadie puede aprobar su propio PR).
- `enforce_admins: true` — **imprescindible**: sin esto la protección no aplica al owner del repo, que es el único que escribe, y la regla queda decorativa.
- Force-push y borrado de `main` bloqueados.
- Status checks obligatorios: **todavía no**. Se configuran una sola vez en #2, apuntando a un job agregado `ci-success`, en vez de listar cada job y tener que reescribir la protección cada vez que uno se renombre.

**Consecuencia a tener en cuenta**: GitHub solo autocierra issues cuando el commit llega a la rama **por defecto** (`main`). Con este flujo, un `Closes #N` en un PR a `dev` **no** cierra la issue al mergear: se cierra cuando `dev` se integra en `main`. Es la semántica correcta (la issue está hecha cuando está en `main`), pero implica ver las issues abiertas durante toda la fase.

### D-007 — El runtime persistente no siembra (seed) el estado inicial del avión
**Fecha**: 2026-07-15 (Fase 1, issue #7)
El `SimulationTestBed` de FBW, tras `Simulation::new`, ejecuta `seed()`: recorre el avión con un `SimulationToSimulatorVisitor` y escribe en el store el estado inicial programado de cada elemento (p. ej. pulsadores que arrancan en ON como `OnOffFaultPushButton::new_on`). Ese paso depende de `Simulation::accept`, que es **privado** en el crate `systems` (`mod.rs:485`), igual que el struct `SimulationToSimulatorVisitor` (`mod.rs:525`). No hay ninguna vía pública para sembrar desde fuera del crate, y la regla del proyecto es no parchear el vendor.
**Consecuencia**: el runtime `Runtime::new(Apron)` arranca **sin seed**; todo pulsador/variable no escrita lee su default (0.0 / OFF). Para el vertical slice eléctrico esto *es* el cold & dark puro: de hecho el spike de Fase 0, que sí usaba el test bed seeded, tenía que forzar `OVHD_ELEC_BAT_{1,2}_PB_IS_AUTO=false` a mano para deshacer el AUTO que el seeding les ponía. Sin seed, ese estado se obtiene de serie. Verificado por test: `Apron` cold & dark deja toda la red sin alimentar, y `battery ON` levanta el DC BAT bus (sin AC), reproduciendo el spike.
**Reevaluar** en fases posteriores si algún subsistema (hidráulico/neumático/APU) resultara depender de un pulsador cuyo estado programado inicial es ON y cuyo default 0 lo dejara en un estado incorrecto (no solo "apagado"). En ese caso, la solución preferida sigue siendo escribir esos pulsadores explícitamente por nombre en el perfil de arranque, no parchear el vendor.

### D-008 — Modelo de errores de la capa API (validación contra el catálogo del registro)
**Fecha**: 2026-07-15 (Fase 1, issue #9)
La capa `api::Sim` valida los nombres de control/variable contra el **catálogo vivo del registro**: tras construir el avión, el `VariableRegistry` contiene todos los nombres que los sistemas leen/escriben más los del entorno, así que es la fuente de verdad de "nombres válidos". `set`/`get` de un nombre desconocido devuelven `ApiError::UnknownControl` **sin acuñar** un identificador (usan `registry.find`, que no muta), evitando que un typo contamine el registro. `set` con valor no finito (NaN/∞) devuelve `ApiError::BadValue`. Motivo: un REPL y un LLM necesitan saber *qué* estuvo mal.
**Desviaciones deliberadas respecto al contrato de `CLAUDE.md`** (registradas aquí como pide el criterio de #9):
- `get` **también** devuelve error en variable desconocida (el contrato solo dice `get(vars) -> dict`). Se prefiere un error explícito y accionable a devolver un 0.0 silencioso; el descubrimiento se hace con `list_variables()`.
- `step`/`run`/`set_environment` devuelven `()` (son infalibles) en vez de `Result`; solo `set`/`get` devuelven `Result`.
- `read_ecam()` y las llamadas de fallos (`inject_failure`/`clear_failure`/`list_failures`) **no** se implementan: son de Fase 2 (#14, #15). Se les deja sitio (el enum de errores y la fachada no cierran la puerta) pero no se stubbean, según indica el propio issue #9.
- `list_controls()` del contrato se pospone; en Fase 1 el descubrimiento lo cubre `list_variables()` (único listado exigido por los criterios de #9).

### D-009 — Catálogo curado de controles (`list_controls`) y validación de rango
**Fecha**: 2026-07-15 (Fase 1, issue #10)
`list_controls()` es la mitad curada del descubrimiento (frente a `list_variables()`, que vuelca el registro crudo). Vive en `core-rs/src/controls.rs`: una constante `CATALOG` **escrita a mano** que mapea nombre amigable → LVAR con metadatos (tipo bool/enum/float, valores válidos, descripción de una línea, grupo por sistema, dominio cabina/mundo). Alcance de Fase 1: panel eléctrico (baterías, ext pwr, APU gen, bus tie, generadores). Los LVAR se tomaron del `A320ElectricalOverheadPanel::new` de FBW y del `ExternalPowerSource`.
**Decisiones de diseño**:
- **Cabina vs mundo** (criterio del issue): se distingue con el enum `ControlDomain` (`Cockpit`/`World`). El único fake de mundo de Fase 1 es `EXT_PWR_AVAIL:1` (simula el GPU enchufado); el resto son pulsadores de cabina.
- **`set` resuelve nombre amigable *y* LVAR crudo**: `set("bat_1", 1.0)` y `set("OVHD_ELEC_BAT_1_PB_IS_AUTO", 1.0)` son equivalentes. Aceptar el LVAR además del nombre amigable mantiene compatible el camino de escritura de #9 y no rompe los tests existentes.
- **Validación en capas**: si el control está en el catálogo, `set` valida el valor contra sus valores válidos (un booleano rechaza cualquier cosa que no sea 0/1) antes de escribir; si no está catalogado, se conserva el comportamiento de #9 (solo finito + existe en el registro). Así la validación de rango del issue #10 convive con la escritura de variables crudas no curadas.
- **`ApiError` no se extendió**: el criterio permitía extenderlo "si hace falta". El valor fuera de rango reutiliza `ApiError::BadValue { name, value, reason }`, cuyo campo `reason` ya transporta el motivo legible ("must be 0 (off) or 1 (on)", "must be within [min, max]"). No hacía falta una variante nueva.
- **Test anti-drift**: `every_catalog_lvar_is_registered_after_a_tick` comprueba que cada LVAR del catálogo aparece en el registro tras un tick; caza typos en el catálogo y renombrados del vendor upstream.
Esto cierra la desviación anotada en D-008 ("`list_controls()` se pospone; en Fase 1 lo cubre `list_variables()`").

### D-010 — Bindings PyO3: crate `bindings/` como workspace independiente, empaquetado con maturin
**Fecha**: 2026-07-15 (Fase 1, issue #11)
El crate `bindings/` (`cdylib` + `rlib`, lib `a320_sim`) expone `api::Sim` como clase Python síncrona vía **PyO3 0.25** (abi3-py39; wheel único válido para CPython ≥ 3.9). Por el FFI solo cruzan `f64`/`bool`/`str`/list/dict; ningún tipo de FBW se filtra. Los `ApiError` afloran como excepciones Python (`SimError` base, con subtipos `UnknownControlError` y `BadValueError`, mensaje del `Display`), nunca panics.

**Decisiones concretas y su porqué**:
- **`#[pyclass(unsendable)]`.** El avión de FBW usa `Rc`/`RefCell` internamente (p. ej. `payload::BoardingInputs`, `electrical::Potential`), así que `Sim` no es `Send` y PyO3 rechaza el `#[pyclass]` por defecto. `unsendable` liga la instancia al hilo Python que la creó; si otro hilo la toca, PyO3 lanza un `RuntimeError` explícito en Python (no un panic por el FFI, no un data race). Para la CLI y el MCP —acceso secuencial desde un hilo— es el contrato correcto. La alternativa (mover la sim a un hilo dedicado con canales) se descartó por complejidad sin beneficio en este uso.
- **Workspace independiente, no miembro del de core-rs.** `core-rs` es package+workspace con `exclude = ["vendor"]` (D-005). Meter `bindings/` como miembro obligaría a un `members`/`..` cruzando directorios y a que su workspace resolviera de nuevo la herencia `workspace = true` del vendor. En su lugar, `bindings/` declara su propio `[workspace]` vacío y depende de `a320-sim-core` por `path = "../core-rs"`. Cada crate es su propia raíz; maturin/cargo resuelven solo `bindings` + `core-rs` + vendor. Es el mismo patrón que ya usa core-rs, por las mismas razones.
- **maturin** como build-backend (PEP 517). Es la opción por defecto y estándar para PyO3; `pip install -e .` en un venv limpio produce el módulo editable `a320_sim`. No se evaluó ninguna alternativa (setuptools-rust) porque maturin cubre el caso sin fricción; el criterio del issue #11 ("si se elige otra cosa, registrar por qué") no aplica.
- **`extension-module` tras feature, no en default.** La feature `pyo3/extension-module` (desacopla la extensión de libpython) la activa maturin al empaquetar (`[tool.maturin] features`), pero se deja fuera de `default` para que `cargo test` enlace libpython del intérprete y compile el binario de tests nativo. Así ambos criterios del issue —`pip install -e .` y `cargo test` nativo— se cumplen sin conflicto de enlazado.
- **Toolchain pineado también en `bindings/`.** `bindings/rust-toolchain.toml` fija 1.93.0 (igual que core-rs y el vendor): este crate compila el vendor transitivamente y maturin invoca cargo en su directorio, así que el pin debe gobernar también aquí. Construir ahora requiere **ambos** toolchains (Rust + Python), documentado en el README.

**Alcance**: el binding es 1:1 con la superficie actual de `api::Sim` (`set`/`get`/`step`/`run`/`set_environment`/`snapshot`/`list_variables`/`sim_time`). `list_controls()` (#10/#12) y los fallos + `read_ecam()` (Fase 2) se añadirán cuando existan en el core; no se stubbean aquí.
**Actualización (#12)**: `list_controls()` ya está expuesto en el binding — devuelve una lista de dicts (`name`, `lvar`, `kind`, `valid_values`, `description`, `group`, `domain`), todo `str` para cruzar el FFI. Cierra la parte que #10/#11 dejaron pendiente por ir en paralelo.

### D-011 — CLI REPL: stdlib (`cmd` + `readline`/`pyreadline3`), sin `prompt_toolkit`
**Fecha**: 2026-07-15 (Fase 1, issue #12)
El REPL humano (`cli/`, paquete `a320_cli`) se construye sobre la **stdlib**: `cmd.Cmd` para el bucle de lectura, el despacho de comandos (`do_*`), la ayuda por comando (docstrings + `help_*`) y el autocompletado con readline (`complete_*`). En Windows `readline` lo aporta `pyreadline3` (dependencia con marcador `platform_system == 'Windows'`); en Linux/macOS CPython ya lo trae. Si falta readline, el REPL sigue funcionando sin tab-completion (aviso al arrancar, no un fallo).
**Por qué stdlib y no `prompt_toolkit`**: la superficie es un REPL de una línea por comando con completado por prefijo de nombres de control/variable; `cmd.Cmd` lo cubre entero sin dependencias nativas ni un bucle async. `prompt_toolkit` aportaría multilínea, resaltado y widgets que aquí no se usan, a cambio de una dependencia pesada. La ergonomía cómoda para la capa de agente (motivo de elegir Python en D-004) es del servidor MCP (Fase 3), no del REPL humano.
**Decisiones de diseño concretas**:
- **Sin lógica de simulación** (principio de "un core, dos frontends"): cada comando es un mapeo 1:1 sobre `a320_sim.Sim`. La CLI no conoce nada de FBW ni del registro salvo por lo que la API le devuelve.
- **`SimError` nunca se propaga como traceback**: todo comando envuelve la llamada al core y `ValueError` de parseo, e imprime una línea `error: ...` accionable (criterio del issue). El mensaje viene del `Display` del `ApiError`, que ya dice cómo descubrir nombres válidos.
- **Valores amigables en `set`**: `on/off`, `true/false`, `yes/no`, `auto` mapean a `1.0/0.0` además de cualquier literal numérico; la validación de rango sigue siendo del core (D-009), la CLI solo traduce el alias. `auto = on` porque los pulsadores de batería/bus tie usan AUTO como su estado "en el bucle".
- **`watch` consciente del TTY**: en un terminal real redibuja las mismas líneas en el sitio (cursor-up + `\033[K`) a ~5 Hz; cuando `stdout` está redirigido (captura/automatización) cae a una línea de log por refresco, sin secuencias ANSI, para que las transiciones se lean limpias. Sale con `Ctrl+C` (KeyboardInterrupt) sin abandonar el REPL. El paso a ~5 Hz (`step 200 ms` + `sleep 0.2 s`) reproduce el patrón de settling del core, así que se ve al DC BAT y a la red AC cobrar vida en tiempo casi real.
- **Empaquetado**: `pip install -e cli/` (setuptools, paquete plano `a320_cli`), console-script `a320-cli` y `python -m a320_cli`. Depende de `a320-sim` (instalado antes desde `bindings/`, no está en PyPI); pip la da por satisfecha si ya está en el venv. GPLv3 por enlazar (vía la extensión) con el vendor de FBW.

### D-012 — Tick de inicialización en `Runtime::new` (refinamiento de D-007; issue #39)
**Fecha**: 2026-07-15 (Fase 1, issue #39)
Escribir los pulsadores de batería **antes** del primer tick dejaba el contactor de batería abierto **para siempre** (el DC BAT bus nunca se alimentaba, sin importar cuánto settling ni re-escrituras). El patrón "tica primero y luego escribe" (el de los tests de integración) funcionaba; el REPL y el MCP, que arrancan en t=0 y cuyo primer comando puede ser `set bat_1 1`, caían de lleno en el caso roto.

**Causa raíz** (estado privado del avión, no una variable del store): el `BatteryChargeLimiter` (`fbw-common/src/wasm/systems/systems/src/electrical/battery_charge_limiter.rs`) arranca en `State::Open` (`:25`, con comentario upstream reconociendo que ese estado inicial no vale para todos los arranques). En cold & dark headless, **ninguna** condición de `Open::should_close` (`:243`) puede llegar a cumplirse:
- La rama de tierra (`on_ground_at_low_speed_with_unpowered_ac_buses`, `:525`) exige `lgciu1.left_and_right_gear_compressed`, y el LGCIU sin alimentar devuelve `false` (`landing_gear/mod.rs:518`: `self.is_powered && …`).
- La rama de carga (`update_begin_charging_cycle_delay`, `:298`) exige el bat bus por encima de 27 V — muerto precisamente porque el contactor no cierra (pescadilla que se muerde la cola).
- La rama de APU exige el APU master ON.

La única vía real hacia `Closed` en tierra es `Open -> Off -> Closed::from_off()` (`:176`): que el pulsador se **lea en OFF al menos un tick** (transición a `Off`, `:332`) y después pase a AUTO (1 s de startup delay, `:176`). Con el patrón "tick primero" eso ocurre de forma natural; si el caller escribe `OVHD_ELEC_BAT_x_PB_IS_AUTO=1` antes del primer tick, el BCL nunca pisa `Off` y queda atascado en `Open` sin salida. Re-escribir el LVAR no ayuda: el latch es la máquina de estados privada, no la variable.

**Fix elegido**: un **tick de inicialización dentro de `Runtime::new`** (`core-rs/src/runtime.rs`, `Runtime::initialize`): un único tick de 100 ms con todos los controles en su default (OFF) ejecutado antes de que el caller pueda escribir nada, y después `sim_time` restaurado a 0. La alternativa del issue (sembrar las variables culpables en el perfil de arranque, la vía preferida por D-007) **no es aplicable aquí**: el estado latcheado es un enum privado del avión que no vive en el store — no hay variable que sembrar. El tick de init es exactamente el resorte que el propio comentario upstream (`:21-24`) echa en falta ("when an initialisation phase is added…"), aplicado desde nuestro lado sin tocar el vendor.

**Efectos y semántica**:
- `sim_time` queda en **0** tras `new()`: el reloj del caller no se adelanta y "tiempo real y monótono desde 0" se conserva. El avión ve dos ticks con `simulation_time=0` (el de init y el primero del caller); todo el razonamiento temporal de los sistemas usa `delta`, no el tiempo absoluto, así que es inocuo.
- El cold & dark de D-007 **no cambia**: tras el tick de init todo sigue en default y la red sigue muerta; lo único que cambia es que las máquinas de estado internas ya han hecho su primera transición coherente con "todo OFF". Los tests existentes pasan sin modificar.
- Se elige 100 ms (delta nominal); ningún retardo interno acumula nada relevante durante la init porque todos los controles están en OFF.

Regresión cubierta por `writes_before_the_first_tick_do_not_wedge_the_battery_contactor` (`core-rs/src/runtime.rs`): el caso B del issue (set antes de todo tick) debe comportarse como el caso A (tick primero).

### D-013 — Catálogo de fallos: ids estables propios sobre `FailureType` (issue #14)
**Fecha**: 2026-07-17 (Fase 2, issue #14)
La inyección de fallos **no necesita parchear el vendor ni pasar por MSFS**. `Simulation::update_active_failures(FxHashSet<FailureType>)` es público (`fbw-common/.../systems/src/simulation/mod.rs:468`) y es el mismo mecanismo que usa el `SimulationTestBed` de FBW (`test.rs:329-339`). El canal de LVAR/CommBus (`FBW_FAILURE_UPDATE`) que trae `systems_wasm` es un detalle de la capa MSFS y queda fuera de nuestro grafo (D-005 intacto).

**El contrato del vendor es declarativo, no un toggle**: cada llamada reemplaza el conjunto activo entero (`Failure::receive_failure` hace `active_failures.contains(&self.failure_type)`). Por eso el dueño del `FxHashSet<FailureType>` es el `Runtime`, y lo reenvía **en cada tick** (`runtime.rs`, dentro de `tick`, junto a `environment.write_all`). Reenviarlo por tick —y no solo al mutar el set— vuelve irrelevante el orden inyectar-antes-de-ticar, que es exactamente la clase de trampa que costó el issue #39 con las baterías. A diferencia de D-012, aquí no hay riesgo en el tick de init: los fallos no viven en el store y el set arranca vacío, que es el estado correcto.

**Los ids son nuestros y a mano** (`core-rs/src/failures.rs`, `CATALOG`), no la forma del enum de FBW. `FailureType` deriva solo `Clone, Copy, PartialEq, Eq, Hash`: **no `Debug`, no `Serialize`, sin id numérico**. No hay nada que exponer directamente a Python ni al MCP, y su forma cambia con el pin. Un id estable (`elec.tr.1`) se puede escribir en un fichero de escenario de Fase 5 y sigue significando lo mismo tras un bump; el mapeo versionado convierte ese bump en un diff visible (o en un fallo de compilación si una variante desaparece) en vez de una renumeración silenciosa.

**Decisiones concretas**:
- **Alcance ATA24 (eléctrico), 20 entradas.** Es el único sistema que la Fase 1 sabe observar. Catalogar ahora los ~50 fallos restantes (aire, hidráulico, tren, RA...) sería catalogar ids que ningún test puede ejercitar: un mapeo equivocado no lo notaría nadie. Se amplía por fase.
- **Campo `ata` copiado de FBW.** La tabla `(u32, FailureType)` de `a320_systems_wasm/src/lib.rs:101-163` es la numeración de FBW; se copia como metadato para poder cruzar cualquier id nuestro con upstream. Es un **dato copiado, no un enlace**: `a320_systems_wasm` no entra en el build nativo.
- **`Debug` de `FailureDef` a mano**, con `finish_non_exhaustive()`: `FailureType` no es formateable. No se pierde nada — lo legible es nuestro `id`, que es justo lo que el enum del vendor no sabe decir de sí mismo.
- **`ApiError::UnknownFailure`** en vez de reutilizar `UnknownControl`: un id de fallo y un nombre de control son espacios de nombres distintos, y el mensaje debe apuntar a `list_failures()`, no a `list_variables()`. En los bindings el `match` de `to_pyerr` es exhaustivo sin `_ =>` a propósito: la variante nueva rompe la compilación justo donde hay que decidir la excepción Python (`UnknownFailureError`).
- **Idempotencia**: inyectar dos veces o limpiar algo no activo son no-ops, no errores. Es la semántica de un conjunto, y le ahorra al agente LLM tener que llevar la cuenta.
- **No existe fallo de batería ni de contactor** en todo el enum de FBW (`battery.rs` no tiene campo `Failure`). Los únicos componentes eléctricos fallables son generadores, TRs, static inverter y buses. Queda documentado en el módulo: el proxy más cercano a "pérdida de batería" es `elec.bus.dc_bat`. No se inventa un id que el vendor no puede honrar.

**Hallazgo del test de integración** (`core-rs/tests/failure_injection.rs`): "inyectar y limpiar devuelve el sistema al estado previo" solo se sostiene para el **estado discreto** de la red (`*_IS_POWERED`, `*_POTENTIAL_NORMAL`). Las magnitudes continuas no vuelven, y es correcto que no vuelvan: `ELEC_BAT_1_CURRENT` refleja que la batería se descargó un poco mientras el TR estaba fallado. Exigir el snapshot entero sería exigir que el avión olvide que el fallo ocurrió.

### D-014 — No hay FWC en el Rust: el catálogo ECAM es nuestro (issue #15)
**Fecha**: 2026-07-17 (Fase 2, issue #15)
Nota de diseño completa con la evidencia: `docs/fase2-ecam.md`.

`CLAUDE.md` anticipaba que `read_ecam()` sería "mapear los warnings del FWC". **No hay FWC en el código vendorizado**: cero coincidencias de `flight_warning`, `FlightWarningComputer`, `master_caution` ni `master_warning` en todo el árbol (`fbw-a32nx`, `fbw-a380x`, `fbw-common`). El propio vendor lo reconoce (`a320_systems/src/surveillance.rs:73`: *"TODO: Comes from FWC"*). Además el ECAM en TypeScript **ni siquiera está vendorizado**: el submódulo está en sparse-checkout (`fbw-a32nx/src/wasm`, `fbw-a380x/src/wasm`, `fbw-common/src/wasm`), así que `fbw-a32nx/src/systems` no existe localmente. Era el riesgo que el propio issue #15 marcaba como abierto; se materializó.

**Consecuencia arquitectónica**: `read_ecam()` es un **motor de reglas nuestro** (`core-rs/src/ecam.rs`) sobre variables que el Rust sí escribe, no un mapeo de un FWC inexistente. Portar el FWC es un subproyecto (y su lógica de inhibición por fase de vuelo es justo lo que no está), y el texto de los mensajes vive en una capa que ni compilamos.

**Decisiones concretas**:
- **`EcamSource` (`VendorFlag` / `Derived`) por regla.** Distingue lo que calcula FBW (la luz FAULT de un pulsador del overhead) de lo que concluimos nosotros (p. ej. "TR alimentado pero sin potencial normal"). **No es cosmético**: es la frontera entre el ground truth heredado y el inventado. La contribución de investigación es el entorno evaluable; si en la Fase 5 no se puede decir qué parte del ground truth es de FBW, no se puede decir qué mide el benchmark. Se registra por regla y aflora hasta la CLI (`[fbw]`/`[ours]`) y el binding.
- **Gate de alimentación.** Sin FWC no hay inhibición, y el flag de AC ESS FEED es `!ac_ess_bus_is_powered` **sin más condiciones**: en cold & dark vale `true` sin ningún fallo (verificado empíricamente, y el propio test de FBW `when_ac_ess_bus_is_unpowered_ac_ess_feed_has_fault` lo afirma). Un mapeo naive daría una caution en un avión sano y violaría el criterio de #15. Las reglas solo se evalúan si la ECAM estaría viva (AC ESS o DC ESS alimentados); si no, lista vacía. No es un parche para pasar el test: en el avión real la ECAM no está alimentada en cold & dark. El criterio del issue y la fidelidad piden lo mismo.
- **Solo lo alcanzable.** Seis reglas eléctricas. El RAT & EMER GEN FAULT queda **fuera y documentado**: su condición exige `!context.is_on_ground()` (`electrical/mod.rs:408`) y toda la Fase 2 es en tierra. Un test (`no_rule_depends_on_being_airborne`) lo recuerda. Tampoco hay BAT FAULT: las baterías nunca reciben `set_fault` en FBW; no está modelado y no se finge.
- **Los TR no tienen luz de fault** (ni en el avión real ni en FBW): sus dos reglas son `Derived`, y su condición exige el bus AC de entrada vivo. Sin eso, un TR sin alimentar se reportaría como averiado — un TR sin AC no está roto, está apagado, y el mensaje falso taparía la causa real.
- **`every_ecam_rule_reads_registered_lvars`** es el anti-drift crítico: si upstream renombrase un `OVHD_*_PB_HAS_FAULT`, la regla quedaría **muda para siempre** (`peek_by_name` → 0.0, el warning nunca salta) y ningún otro test lo notaría — todos verían "ECAM limpia", que es lo esperado sin fallos.

**Nota sobre el seeding**: la exploración advirtió de que `ENG_GEN_{1,2}_PB_HAS_FAULT` también daría falso positivo en cold & dark porque esos pulsadores arrancan en ON en FBW (`new_on`). Eso vale para el test bed *seeded*; **nuestro runtime no siembra** (D-007), así que leen 0 = OFF y no dan fault. Verificado empíricamente. La trampa nos llega solo vía AC ESS FEED, que no depende de ningún pulsador.

### D-015 — Servidor MCP: FastMCP v1 sobre stdio, tools síncronos y un solo hilo (issue #51)
**Fecha**: 2026-07-17 (Fase 3, issue #51)
Servidor en `mcp/` (paquete Python **`a320_mcp`**, no `mcp`: el SDK oficial se importa exactamente así y el paquete lo ensombrecería). Sobre los bindings de D-010, con el SDK oficial: `from mcp.server.fastmcp import FastMCP`, transporte **stdio** (el default de `mcp.run()`).

**Pin `mcp>=1.28,<2`**: la 1.28.1 es la estable; la 2.0 está en alfa y **ya cambió la API** (su README documenta `MCPServer` en vez de `FastMCP`). El propio PyPI recomienda el tope explícito antes de que salga la 2.0 estable. El SDK exige **Python ≥ 3.10**, así que este paquete sube el piso (bindings y CLI siguen en ≥ 3.9); solo afecta a `mcp/`.

**Los tools son funciones `def` síncronas, y eso es carga estructural, no estilo.** FastMCP llama a un tool síncrono **inline en el hilo del event loop** — `func_metadata.py`: `if fn_is_async: return await fn(...)` / `else: return fn(...)`, sin `anyio.to_thread` ni executor. Eso es exactamente lo que el `Sim` necesita: es `unsendable` (D-010, el avión usa `Rc`/`RefCell`) y tocarlo desde otro hilo lanza `RuntimeError`.

**Consecuencia que hay que aceptar a conciencia**: `advance(60)` bloquea el event loop ~20 s de reloj. Es correcto y deliberado — con stdio hay un solo cliente y no hay nada más que servir. La "optimización" evidente (mandarlo a `anyio.to_thread` para no bloquear) es precisamente lo que rompería el binding. Va comentado en `advance`, porque es una trampa que solo se ve sabiendo lo del `Rc`/`RefCell`. La afirmación contraria se llegó a hacer de memoria antes de leer el SDK; ver **L-001..L-005** (`docs/lecciones.md`), lección L-005.

**Un `Sim` por proceso, construido en import** (no en `main()`): los esquemas embeben los catálogos y los decoradores corren en import (ver D-017). Cuesta ~1 s, que se pagaría igual al arrancar.

### D-016 — Lo que el agente NO puede ver es parte del contrato (issue #51)
**Fecha**: 2026-07-17 (Fase 3, issue #51)
El binding expone `active_failures()` y `list_variables()`. **Ninguno de los dos se expone como tool**, y no por olvido — es una decisión de diseño del benchmark:

- **`active_failures()` filtraría el ground truth.** El agente debe diagnosticar desde la ECAM, como un piloto. Un tool que le diga "está roto `elec.apu_gen.1`" convierte el benchmark en un test de comprensión lectora en vez de uno de diagnóstico. Esto es lo que mide la Fase 5; exponerlo la invalidaría.
- **`list_variables()` son cientos de nombres**: ahogaría la ventana de contexto, que es justo el recurso que el issue #17 pedía cuidar.

**Consecuencia**: sin `list_variables`, **`snapshot(contains=...)` es el mecanismo de descubrimiento de salidas** — el agente no puede adivinar `ELEC_AC_1_BUS_IS_POWERED` desde `list_controls`, que solo cataloga *entradas*. Eso convierte la descripción de `snapshot` en carga estructural (sugiere los prefijos por sistema), no en un docstring.

Ambas omisiones están protegidas por un test (`test_the_agent_cannot_see_the_ground_truth`): las dos funciones están a una línea de que alguien las añada al verlas en el binding y suponer que faltaban.

**Salidas acotadas** (criterio del issue): `read_state` toma lista; `snapshot` exige filtro y **rechaza** un filtro que casa demasiado (>60 vars) en vez de volcarlo; `advance` capa `seconds` a 600 — sin tope, un `advance(100000)` cuelga el servidor y al agente le parece que el avión se rompió.

**`ToolAnnotations` declara la semántica real** de cada tool: los cinco de lectura son `readOnlyHint`; `inject_failure` es `destructiveHint` (rompe un sistema, reversible) e **idempotente** — que es exactamente la semántica de conjunto de D-013; `advance` es el único **no** idempotente (el tiempo corre); y los nueve son `openWorldHint: False` porque el simulador es un mundo cerrado.

### D-017 — Esquemas generados desde los catálogos; escenario montado por el arnés (issue #51)
**Fecha**: 2026-07-17 (Fase 3, issue #51)
**Los nombres válidos viajan en el esquema como `enum`**, generados en import desde `list_controls()`/`list_failures()` con un `Literal` dinámico. Verificado empíricamente antes de comprometerse (era el riesgo abierto del plan): pydantic produce `{"enum": ["apu_gen","bat_1",...], "type":"string"}` para `set_control.control` y los 21 ids para `inject_failure.failure_id`. No hizo falta el fallback previsto (`Field(json_schema_extra=...)`). Así el modelo **no puede alucinar un nombre**, y la fuente sigue siendo el catálogo: cero duplicación.

**Solo nombres amigables, no LVARs crudos**: es la mitad curada del descubrimiento haciendo su trabajo (D-009). El agente acciona controles de cabina que un humano curó. Si un escenario necesita un control que no está, la vía es **catalogarlo en `core-rs/src/controls.rs`**, no ensanchar el enum. (Cuando se escribió esto dejaba fuera los pulsadores del APU, solo accionables por LVAR crudo; desde el slice 2 de Fase 4 (#56) están catalogados como `apu_master`/`apu_start`/`apu_bleed` y el arnés del MCP los usa por nombre amigable.)

**`--start cold-dark|apu-running`**: el escenario lo monta el **arnés**, no el agente. `apu-running` reusa la secuencia exacta del test de #16 (`UNLIMITED FUEL`, baterías, master + start, espera acotada a la turbina, `apu_gen` ON, **sin ext pwr** porque la condición del fault lo exige) y entrega el avión listo. (Desde el slice 3 de Fase 4 (#57), `UNLIMITED FUEL` se retiró de la secuencia: el fuel viene del seed por defecto del runtime — ver D-018.) Motivo: el demo mide "sabe gestionar el fallo", no "sabe arrancar un APU" (cuando se escribió esto, además, los pulsadores del APU no estaban catalogados y el agente ni podría descubrirlos; desde #56 sí lo están, pero el motivo principal sigue en pie). Es además la costura natural hacia los escenarios de la Fase 5.

**Las descripciones de los tools y las `instructions=` del servidor son prompt engineering**, no documentación: son lo único que el modelo sabe de un avión que no puede ver, y en la Fase 5 son un eje de ablación. La advertencia más importante que llevan es que **el tiempo no corre solo**: un control escrito no hace nada hasta llamar a `advance`, y un agente que lea el estado justo después de actuar verá el estado anterior y concluirá que su acción no sirvió.

### D-018 — Combustible como estado de mundo sembrado una vez (issue #57)
**Fecha**: 2026-07-21 (Fase 4, slice 3)
El Rust de FBW **no modela consumo ni crossfeed**: los simvars `FUEL TANK * QUANTITY` son *entradas* de mundo en galones US que `FuelTank::read` convierte a kg con `FUEL_GALLONS_TO_KG = 3.039075693483925` (`fbw-common/.../systems/src/fuel/mod.rs:12,97-100`), y las bombas `FUELSYSTEM PUMP ACTIVE:{id}` solo alimentan el consumo eléctrico (`FuelPump`, `:202-241`; el `consume_power` está en `:232-240`). Consecuencias de diseño:

- **Seed, no entorno**: `Runtime::new` escribe la carga por defecto **una sola vez**, antes del tick de inicialización (`FUEL_SEED_GALLONS` en `runtime.rs`). Si el entorno la reescribiese cada tick, ningún escenario podría vaciar un tanque — y vaciar el left main con el APU corriendo es exactamente el escenario que el slice habilita (`ApuFault::FuelLowPressure`, `electronic_control_box.rs:224-230` → caution "APU FAULT").
- **Reparto** (~6 400 kg, carga de bloque de corto radio): el repostaje real del A320 llena las alas antes que el central — aux (outer) llenos (228 gal), el resto a partes iguales en los mains (825 gal), center vacío. 2 106 gal ≈ 6 400 kg. Capacidades de `A320_FUEL` (`a320_systems/src/fuel/mod.rs:53-79`); ojo al wording MSFS: `LeftInner` de FBW = `FUEL TANK LEFT MAIN QUANTITY`.
- **Catálogo**: grupo `Fuel`, dominio **World** entero (tanques con rango 0..capacidad, `unlimited_fuel`, bombas) — no hay pulsadores de fuel en el Rust del vendor, así que nada de esto es cabina.
- **Muleta `UNLIMITED FUEL` retirada** de `_start_apu_running` (MCP), `generator_caution.rs`, `apu_slice.rs` y la demo de `main.rs`: el APU arranca con el fuel sembrado. El flag sigue catalogado (`unlimited_fuel`) por si un escenario futuro necesita explícitamente el caso ilimitado, pero ningún camino nuestro lo escribe (lo vigila `tests/fuel_slice.rs`).

### D-019 — Motor propio: spool de N2 de primer orden (issue #58)
**Fecha**: 2026-07-21 (Fase 4, slice 4)
El Rust de FBW no modela el motor (el spool vive en el FADEC C++/WASM y en MSFS): los sistemas solo **leen** simvars de motor como entrada pura. Headless hay que generarlas, y se eligió un **modelo propio con spool de N2 de primer orden por tramos** (`core-rs/src/engine.rs`: `n2 += (target - n2)·(1 - e^(-dt/τ))`, determinista en función del `dt` del tick — nada de reloj de pared ni azar, requisito del benchmark). Timing resultante: arranque ~50 s hasta idle (58.5 % N2), spool-down ~49 s. Constantes y contrato completo en `docs/fase4-motor.md`.

Alternativas rechazadas:
- **Fijo-por-régimen** (escribir N2=58.5 al instante): el arranque ES el deliverable del slice, y saltar de 0 a idle no ejercitaría nada de lo que el vendor deriva del transitorio — la válvula de arranque (abre con `Starting` y N2 < 65 %, `a320_systems/src/pneumatic.rs:458-473`), el cruce de presión de aceite a 25 % de N2 (`fbw-common/.../engine/leap_engine.rs:67-68`), la lógica de PTU-inhibit con un solo master, o un fallo inyectado a mitad de arranque (Fase 5).
- **JSBSim**: dependencia nativa pesada, riesgo de no-determinismo entre plataformas, y una fidelidad que nadie consume — el vendor solo lee estas pocas simvars; toda la interacción entre sistemas (lo que este proyecto evalúa) ya la pone FBW.

### D-020 — Contrato de arranque de motores (issue #58)
**Fecha**: 2026-07-21 (Fase 4, slice 4)
Las dos mitades del contrato, verificadas contra los consumidores del vendor (rutas relativas a `core-rs/vendor/aircraft`):

- **Inputs de cabina**: `ENG_MASTER_{1,2}` son LVARs **nuestros** — en MSFS el engine master vive en el fuel system C++ y ningún elemento del Rust del vendor lo registra; el runtime los siembra a 0 en `Runtime::new` (`ENGINE_CONTROL_SEED`). El selector de modo sí es del vendor: `TURB ENG IGNITION SWITCH EX1:1`, **un único selector para ambos motores**, leído por el FADEC de pneumatic (`a320_systems/src/pneumatic.rs:1608-1609`) con el enum `EngineModeSelector` CRANK=0/NORM=1/IGN-START=2 (`fbw-common/.../pneumatic/mod.rs:764-782`). Se siembra a **1 (NORM)**: sin seed, una var no escrita lee 0.0 = CRANK, que no es el reposo del panel real.
- **`ENGINE_STATE:{n}` lo transicionamos nosotros**: nadie lo escribe en el Rust del vendor fuera de su test bed. Consumidores: el FADEC de pneumatic (`a320_systems/src/pneumatic.rs:1587-1650`, de donde sale la válvula de arranque) y el aire acondicionado, que **no lee el simvar**: recibe el estado vía el trait `EngineStartState` que implementa `A320Pneumatic` delegando en su FADEC (`a320_systems/src/pneumatic.rs:388-394`; firma del update en `a320_systems/src/air_conditioning.rs:70-79`). Se reutiliza el enum `EngineState` del vendor (Off=0/On=1/Starting=2/Restarting=3/Shutting=4, `fbw-common/.../pneumatic/mod.rs:507-528`) para que los valores no puedan divergir; `Restarting` no se produce (fuera de alcance).
- **Outputs por motor y tick**: `ENGINE_N2:{n}` y `TURB ENG CORRECTED N1/N2:{n}` en percent, `TURB ENG JET THRUST:{n}` en libras (el framework lee `Ratio` como percent y `Mass` como pound, `fbw-common/.../simulation/mod.rs:774,781`; lector: `LeapEngine`, `leap_engine.rs:42-45,72-78`). Corrected = uncorrected (tierra, ISA).
- **`GENERAL ENG STARTER ACTIVE:{n}` espeja el master, no el corte del starter**: su único lector Rust es el controlador del PTU y lo trata como *eng master on/off* (`a320_systems/src/hydraulic/mod.rs:3449-3452,3550-3554`; sus campos se llaman `eng_{n}_master_on`), y el test bed del vendor lo mantiene a 1 mientras el motor corre (`hydraulic/mod.rs:7145-7183`). El corte real del starter ya lo modela el vendor con la válvula de arranque a 65 % de N2, alimentada por nuestros `ENGINE_STATE`/`ENGINE_N2`. (El briefing del slice proponía cortarlo a 50 % durante `Starting`; se descartó con el vendor delante — habría roto la lógica de PTU-inhibit, que necesita ver el master, no el starter.)
- **Gate de bleed (añadido en el slice 5, #59 — 2026-07-22)**: el motoring exige aire real. El stretch del plan **funcionó**: el neumático del vendor calcula headless `PNEU_ENG_{n}_STARTER_PRESSURIZED` (histéresis 10/5 psi sobre ambiente sobre la presión real del contenedor del starter, `a320_systems/src/pneumatic.rs:1278-1288`, write en `:1438-1441`), la válvula de arranque abre con nuestro `ENGINE_STATE = Starting` y N2 < 65 % (`:458-473`), y el ducto solo se presuriza con el APU bleed soplando aguas arriba (50 psi con la turbina en marcha, `fbw-common/.../apu/aps3200.rs:422-425`; ~25 psi medidos en el contenedor). Nuestro `EngineModel` lee ese flag y, sin aire, el tramo de motoring apunta a 0 sin abortar la secuencia (la válvula sigue abierta, listo para cuando llegue el aire); pasado el light-off el gate ya no aplica (combustión autosostenida). Verificado end-to-end: sin `apu_bleed` el N2 queda clavado en 0 con `Starting` armado; con bleed el arranque completa (~55 s). **Coste asumido**: el instante de llegada del aire hereda el azar real del APU del vendor (flap de admisión sorteado 6-12 s, `apu/air_intake_flap.rs:21-31`), así que el test de determinismo del benchmark ancla la comparación en el primer tick de motoring — desde ahí, igualdad f64 exacta (D-019 intacta).

### D-021 — Reposos de panel sembrados y catalogados: GEN 1 LINE, X BLEED, pushback (issue #59)
**Fecha**: 2026-07-22 (Fase 4, slice 5)
El slice de cross-system destapó una familia entera de la misma trampa de D-007 (sin `seed()` del vendor, todo LVAR no escrito lee 0.0): pulsadores/selectores cuyo **reposo real no es 0**, y cuyo 0 significa otra cosa en silencio. La decisión es **sembrar el reposo una vez en `Runtime::new` (`PANEL_RESTING_SEED`/`WORLD_STATE_SEED`, `runtime.rs`) Y catalogarlos como controles** — el seed fija el estado del panel de un avión aparcado, el catálogo los deja accionables (el mismo doble tratamiento que ya tenía `eng_mode`, sembrado a NORM y catalogado):

- **GEN 1 LINE** (panel EMER ELEC): el vendor lo construye `OnOffFaultPushButton::new_on(context, "EMER_ELEC_GEN_1_LINE")` (`a320_systems/src/electrical/mod.rs:391`) — reposo ON; solo se apaga en el procedimiento SMOKE. `gen_1_provides_power` lo exige además del pulsador GEN 1 (`electrical/alternating_current.rs:433-436`; el GEN 2 no tiene equivalente, `:437-439`), así que sin seed el GEN 1 giraba a 115 V/400 Hz con el contactor abierto y `ENG 1 GEN FAULT` en un avión sano. Se optó por seed **y** catálogo (`gen_1_line`) en vez de solo catálogo: obligar a cada escenario a "descubrir" un pulsador que la tripulación real jamás toca en un arranque normal habría sido infidelidad al cockpit, y el seed reproduce exactamente lo que el `seed()` inaccesible del vendor haría (`new_on`). Como control sigue siendo relevante: es la palanca del procedimiento SMOKE (escenario futuro).
- **Selector X BLEED** (panel neumático): `CrossBleedValveSelectorKnob::new_auto`, LVAR `KNOB_OVHD_AIRCOND_XBLEED_Position`, enum SHUT=0/AUTO=1/OPEN=2 (`fbw-common/.../pneumatic/mod.rs:462-470,487-491`) — reposo AUTO. Sin seed leía 0 = **SHUT** y la válvula de crossbleed jamás abría (en AUTO abre con la válvula de APU bleed, `a320_systems/src/pneumatic.rs:986-1008`): el motor 2 no podía arrancar con aire del APU. Sembrado a AUTO y catalogado (`xbleed`, grupo nuevo `Pneu`).
- **`PUSHBACK STATE` (mundo)**: `PushbackTug` trata **3 = sin pushback**; cualquier otro valor — incluido el 0.0 por defecto — es "pushback en curso" e inserta el bypass pin del morro (`fbw-common/.../hydraulic/pushback.rs:24-31,60-69`), lo que inhibía el PTU con un solo master ON *aunque el freno estuviera suelto* (la rama `!parking_brake && !bypass_pin`, `a320_systems/src/hydraulic/mod.rs:3491-3497`). Sembrado a 3 como estado de mundo (seed y no entorno, por el mismo motivo que el fuel en D-018: un escenario futuro de pushback debe poder escribirlo).
- **`park_brake` catalogado sin seed**: la palanca real (`PARK_BRAKE_LEVER_POS`, leída por el controlador del PTU en `hydraulic/mod.rs:3448,3551`) es la que decide el PTU-inhibit con un solo master. Se cataloga (grupo Hyd) porque sin ella el escenario del inhibit no es operable, pero **no** se siembra a 1 (el reposo real de un avión aparcado): cambiar el default del freno movería el baseline de todos los slices previos y arrastraría el sistema de frenos, que nadie ha verificado todavía — se registra como deuda consciente para el slice de frenos.
- **Bombas EDP 1/2** (panel HYD): `AutoOffFaultPushButton::new_auto(context, "HYD_ENG_{1,2}_PUMP")` (`a320_systems/src/hydraulic/mod.rs:4500-4501`) — reposo AUTO, guardadas, la tripulación no las toca en un arranque normal. Sin seed leían 0 = OFF y un motor al ralentí no presurizaba su circuito hasta accionar un pulsador que en el avión real ya está en AUTO (lo destapó la verificación cruzada del slice: los tests lo compensaban a mano con `set hyd_eng_{1,2}_pump 1`). Sembradas a AUTO; ya estaban catalogadas desde el slice 1.
- **Deuda anotada, no resuelta**: los pulsadores ENG 1/2 BLEED (`AutoOffFaultPushButton::new_auto`, `a320_systems/src/pneumatic.rs:1547-1548`) tienen la misma trampa (reposo AUTO, leen 0 = OFF sin seed). No se siembran ni catalogan aún: ningún camino de este slice los necesita (el starter bebe aguas arriba de la PRV) y la regla del repo es no catalogar lo que ningún test ejercita. Le tocará al slice neumático. Igual las bombas eléctricas azul/amarilla y el PTU (reposo AUTO): su seed movería escenarios de Fase 1-2 ya fijados (la amarilla invertida es la convención D-007 de los helpers) — revisar con la secuencia end-to-end (#60).

## Hitos

### Fase 1 cerrada — 2026-07-15
Criterio de éxito cumplido y automatizado: cold & dark → baterías ON → ext pwr con la red cobrando vida, como test de integración (`core-rs/tests/electrical_slice.rs`) y operable a mano en el REPL (`a320-cli`, con `watch`). Entregado en los PRs #29 (readme), #30/#34/#32/#33 (runtime + API, issues #6–#9), #36 (catálogo, #10), #37 (bindings PyO3, #11), #35 (test de integración, #13), #38 (CLI, #12) y #40 (fix del wedge del primer tick, #39, encontrado en la verificación final). Decisiones asociadas: D-007 a D-012. Pin del vendor intacto (`13bce4b`), cero parches al código de FBW. Siguiente: Fase 2 (failures + `read_ecam`, issues #14–#16).

### Fase 2 cerrada — 2026-07-17
Criterio de éxito cumplido y automatizado: **tirar un generador y ver aparecer su caution** (`core-rs/tests/generator_caution.rs`), operable a mano en el REPL (`fail elec.apu_gen.1` + `ecam`) y verificado en CI sobre la demo. El bucle que justifica el proyecto está cerrado: algo se rompe y el avión lo dice.

Entregado en los PRs #47 (inyección de fallos, #14), #48 (`read_ecam`, #15) y #49 (demo del generador, #16). Decisiones asociadas: D-013 (ids estables de fallos) y D-014 (no hay FWC: el catálogo ECAM es nuestro). Pin del vendor intacto (`13bce4b`), cero parches al código de FBW.

**El caso del demo es el APU GEN**, no un generador de motor: el arranque de motores es de Fase 4, así que `Generator(1)/(2)` no son ejercitables (sin motor girando su contactor está abierto de todos modos y el fault no distinguiría un fallo de un estado normal). El APU sí arranca en tierra —y sin arrastrar el sistema de fuel, porque el Rust de FBW no quema combustible: basta `UNLIMITED FUEL`—, y su fault es además el único flag eléctrico correctamente gateado por el estado real del sistema (`apu.is_available()`), así que no da falsos positivos. Satisface el criterio al pie de la letra: es un generador de verdad.

**Hallazgo del escenario**: al caer el APU GEN (única fuente AC) la ECAM levanta **dos** cautions — `APU GEN FAULT` y, aguas abajo, `AC ESS BUS FAULT`. Ambas correctas, y es lo que hace el escenario realista: un agente tendrá que lidiar con la cascada, no con un mensaje aislado. La ECAM sigue legible porque las baterías mantienen vivo el DC ESS; por eso el gate de D-014 mira AC ESS **o** DC ESS. Si mirase solo el AC, este escenario —perder toda la red AC— se quedaría mudo justo cuando más importa.

**Siguiente**: Fase 3 (servidor MCP, issue #17). La superficie que expone (`set`/`get`/`step`/fallos/`read_ecam`/descubrimiento) ya está completa y probada en los bindings; la Fase 3 es sentar a un LLM en la silla.

### Fase 3 cerrada — 2026-07-17
Criterio de éxito cumplido: **un LLM resolvió el fallo del APU GEN usando solo los tools**. Partiendo del escenario `--start apu-running` con la ECAM limpia, inyectado el fallo, el agente leyó la ECAM (`APU GEN FAULT` + `AC ESS BUS FAULT`), diagnosticó desde el estado (toda la red AC muerta; DC 1/2 caídos con ella porque se alimentan vía TR; baterías sosteniendo DC BAT/ESS/HOT), descartó los generadores de motor (no hay motores girando), y ejecutó el procedimiento: APU GEN pb OFF → GPU → EXT PWR ON → `advance` → ECAM limpia y red entera recuperada.

Entregado en los PRs #52 (servidor, #51) y #NN (demo, #17). Decisiones: D-015 (FastMCP v1 sobre stdio, tools síncronos en un solo hilo), D-016 (qué NO se expone), D-017 (esquemas desde los catálogos; escenario montado por el arnés). Lección L-005. `core-rs` sin tocar: pin del vendor `13bce4b` intacto.

**Lo que el demo demuestra y lo que no.** Demuestra que el entorno **sabe plantear un problema resoluble y observable**: hay una cascada real, la ECAM la reporta, y el bucle observar→razonar→actuar→avanzar se cierra por el protocolo real. **No** es un baseline: quien lo condujo ya había visto que ext pwr recupera la red al verificar el ejemplo del README, así que no era un sujeto ciego. La evaluación ciega con ≥2 modelos es la Fase 5, y es justo la distinción que separa "demo" de "benchmark".

**Hallazgo del demo — `ext_pwr_avail` es `domain: world` y estaba en manos del agente.** Para recuperar la red, el agente tuvo que "enchufar la GPU" él mismo. En el avión real la tripulación no puede: pediría una GPU y alguien la enchufaría. La distinción cabina/mundo de D-009 existe precisamente para marcar esto, pero el servidor expone los dos dominios por igual. **Consecuencia para la Fase 5**: un escenario debe fijar su estado de mundo por adelantado (como hace `--start`) y **no** ofrecer los controles `world` al agente, o estará midiendo si el agente adivina que puede alterar el mundo exterior en vez de si sabe el procedimiento. Es exactamente la clase de detalle que solo aparece conduciendo el bucle, no leyéndolo.

**Siguiente**: Fase 4 (#18, hidráulico + APU + fuel + arranque de motores). El APU es el candidato más barato: ya está probado y arrancable, pero sus pulsadores solo son accionables por LVAR crudo — catalogarlos cerraría el hueco que la tabla de sistemas dejó ver.

### Fase 4 cerrada — 2026-07-22
Criterio de éxito cumplido y automatizado: **la secuencia completa cold & dark → engines running**, solo con nombres amigables del catálogo, como test de integración con un checkpoint verificado tras cada paso (`core-rs/tests/cold_dark_to_engines_running.rs`), reproducible a mano en el REPL y entregable a un LLM con `a320-mcp --start engines-running` (probado por el protocolo real en `mcp/tests/test_server.py`). Estado final: ambos motores al ralentí (N2 ~58.5 %), red AC entera por los ENG GEN (sin APU GEN ni ext pwr), verde y amarillo a ~3000 psi por las EDPs, azul por su bomba eléctrica en AUTO, PTU habilitado sin transferir (presiones parejas), APU apagado y **ECAM completamente limpia**.

Entregado en los PRs #61 (panel hidráulico, #55), #62 (controles del APU, #56), #63 (fuel como estado de mundo, #57), #64 (modelo de motor propio, #58), #65 (cross-system de motor, #59) y #66 (secuencia end-to-end, #60). Decisiones asociadas: D-018 (fuel sembrado una vez), D-019 (motor propio: spool de N2 de primer orden), D-020 (contrato de arranque + gate de bleed) y D-021 (reposos de panel sembrados y catalogados). Pin del vendor intacto (`13bce4b`), cero parches al código de FBW.

**Timing medido de la secuencia** (5 Hz, una corrida concreta — varía unos segundos entre corridas por el azar real del APU del vendor, p. ej. el flap de admisión sorteado 6-12 s): AVAIL del APU a ~55 s del START, cada motor a idle en ~50 s desde que su starter recibe aire, y el apagado ordenado del APU (cooldown de bleed + spool-down) ~116 s tras MASTER OFF — ~340 s de simulación en total, ~90 s de reloj en build de debug (segundos en release, que es lo que carga el binding del MCP). Las ventanas de assert del test cubren la varianza.

**Del cierre quedan dos notas.** (1) El bus tie es la pieza que un operador puede olvidar: sin él el APU GEN no alimenta AC 1/2 y el BMC del crossbleed se queda muerto — el arranque del motor 2 depende de un pulsador del panel eléctrico; las `INSTRUCTIONS` del MCP lo avisan como regla de pulgar. (2) La deuda D-021 sigue viva: las bombas eléctricas azul/amarilla y el PTU descansan en AUTO en el avión real pero el runtime no los siembra — la secuencia los pone a mano en la preparación de panel (tres `set` antes de las baterías). Sembrarlos movería los baselines de Fase 1-2; le toca a la Fase 5 decidir si el reposo pasa a ser el default de escenario.

**Siguiente**: Fase 5 (benchmark; épicas #19 y #20): suite de escenarios con ground truth QRH, scoring de cumplimiento a nivel de trayectoria, baselines y ablations.

## Abiertas

*(ninguna)*

## Parches al código vendorizado de FBW

*(ninguno todavía — cada stub/shim/parche necesario para el build nativo se documenta aquí con archivo y motivo)*
