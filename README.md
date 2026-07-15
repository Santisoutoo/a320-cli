# A320 Systems Twin — CLI + MCP para agentes LLM

Simulador **headless de los sistemas del A320** (sin MSFS ni X-Plane corriendo), construido sobre el código open-source de [FlyByWire](https://github.com/flybywiresim/aircraft), expuesto de dos formas: una **CLI** para que un humano opere el avión desde terminal (set switches, leer estado, avanzar tiempo, inyectar fallos) y un **servidor MCP** para que un LLM opere el avión en bucle cerrado: observa (ECAM + estado) → decide → actúa → avanza → observa.

El objetivo final es un entorno reproducible para **detección y gestión de fallos** siguiendo procedimientos reales (ECAM/QRH), pensado como **benchmark de agentes LLM** (research/paper). La contribución de investigación no es el modelo del avión (es de FBW), sino el entorno evaluable + la suite de escenarios + el scoring de cumplimiento de procedimiento.

La lógica de los sistemas (eléctrico, hidráulico, neumático, fuel, APU, presurización, FWC…) se reutiliza tal cual de los crates Rust de FBW, vendorizados y pineados a un commit concreto. Este repo construye todo lo de alrededor: el harness headless persistente, el registro de variables, la API de control/observación, la CLI, el MCP y (más adelante) los escenarios de fallo con su ground truth. Detalles en [CLAUDE.md](CLAUDE.md).

## Estado

**Fase 0 en curso** — spike de viabilidad: compilar los crates `systems` + `a320_systems` de FBW como binario nativo, instanciar el A320, avanzar 1 s de simulación en tierra y leer una variable eléctrica. Las fases están definidas en [CLAUDE.md](CLAUDE.md) (sección Milestones).

## Bootstrap

1. **Instalar Rust** (si no está):

   ```powershell
   winget install Rustlang.Rustup
   ```

   En Windows, rustup usa el target MSVC: si faltan las **Visual Studio Build Tools (C++)**, el propio rustup lo indica durante la instalación (`winget install Microsoft.VisualStudio.2022.BuildTools`). El toolchain concreto (Rust 1.93.0) se auto-instala al compilar gracias a `rust-toolchain.toml`.

2. **Vendorizar FBW** (submódulo pineado, con clone eficiente blob-filter + sparse):

   ```powershell
   .\scripts\bootstrap-vendor.ps1
   ```

3. **Compilar y correr el spike**:

   ```powershell
   cd core-rs
   cargo run
   ```

   La primera compilación tarda varios minutos (descarga y compila las dependencias del monorepo de FBW).

## Licencia

**GPLv3** — heredada de los crates de FlyByWire que este proyecto vendoriza y enlaza.

## Decisiones de arquitectura

Registradas en [docs/decisiones.md](docs/decisiones.md) (pin de FBW, hallazgos de la Fase 0, decisión abierta PyO3 vs Rust-puro…).
