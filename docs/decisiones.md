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

## Abiertas

*(ninguna)*

## Parches al código vendorizado de FBW

*(ninguno todavía — cada stub/shim/parche necesario para el build nativo se documenta aquí con archivo y motivo)*
