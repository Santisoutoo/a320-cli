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

## Abiertas

*(ninguna)*

## Parches al código vendorizado de FBW

*(ninguno todavía — cada stub/shim/parche necesario para el build nativo se documenta aquí con archivo y motivo)*
