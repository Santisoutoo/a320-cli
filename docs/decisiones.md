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

## Abiertas

*(ninguna)*

## Parches al código vendorizado de FBW

*(ninguno todavía — cada stub/shim/parche necesario para el build nativo se documenta aquí con archivo y motivo)*
