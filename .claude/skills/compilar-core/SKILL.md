---
name: compilar-core
description: |
  This skill should be used when the user asks to compile, build, test, or check the Rust simulation core. Trigger when the user says "compila", "compila el core", "build", "cargo build", "¿compila?", "corre los tests del core", "check".
version: 1.0.0
allowed-tools:
  - Bash
  - PowerShell
  - Read
  - Grep
---

# Skill: Compilar el core Rust

## Proceso

1. **Localiza el crate**: el core vive en `core-rs/` (workspace con los crates de FBW vendorizados). Si no existe todavía, informa de que la Fase 0 no ha empezado y para.

2. **Compila en orden de coste**:

   ```bash
   cd core-rs && cargo check 2>&1
   ```

   Si `check` pasa y el usuario pidió build o tests:

   ```bash
   cargo build 2>&1
   cargo test 2>&1
   ```

   Compilación **nativa siempre** — jamás pasar `--target wasm32-*`. Si el build intenta arrastrar `msfs-rs`, `systems_wasm` o wasm-bindgen, es un bug de decoupling: repórtalo como tal, no sugieras instalar toolchains wasm.

3. **Filtra la salida** — solo lo importante:
   - `error[...]` y `error:` → críticos, con `archivo:línea` y explicación breve.
   - `warning:` **solo en nuestro código** (fuera del directorio vendorizado de FBW). Los warnings del código vendorizado se ignoran y no se "arreglan".
   - En tests: lista de tests fallidos con su assert.

4. **Informa el resultado**:
   - **Éxito**: confirma qué compiló (check/build/test), número de tests pasados y tiempo.
   - **Error**: cada error con ubicación y sugerencia de corrección.

## Problemas comunes

- **`msfs-rs` en el árbol de dependencias del build nativo** → falta gatear `systems_wasm` tras un feature flag o el workspace incluye crates de más. Ver `docs/decisiones.md` por si ya hay un stub documentado.
- **Símbolos del harness de FBW no encontrados** → el harness de tests (`SimulationTestBed`) puede estar tras `#[cfg(test)]`; nuestro runtime necesita su propio wrapper, no importar el de tests.
- **El pin del submódulo cambió** (`git submodule status` muestra `+`) → no arreglar errores causados por un pin movido; restaurar el pin (`git submodule update --init`) y recompilar.
