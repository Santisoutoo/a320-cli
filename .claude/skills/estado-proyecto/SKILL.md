---
name: estado-proyecto
description: |
  This skill should be used when the user asks about project progress or current phase. Trigger when the user says "estado", "¿en qué fase estamos?", "progreso", "status", "¿qué falta?", "¿cómo va el proyecto?".
version: 1.0.0
allowed-tools:
  - Bash
  - PowerShell
  - Read
  - Glob
  - Grep
---

# Skill: Estado del proyecto

Informa en qué fase (0–5) está el proyecto y qué falta para cerrar la actual. Las fases están definidas en `CLAUDE.md` (sección Milestones).

## Proceso

1. **Inspecciona el repo**:
   - ¿Qué directorios del layout existen? (`core-rs/`, `bindings/`, `cli/`, `mcp/`, `scenarios/`, `docs/`)
   - ¿Está vendorizado FBW? (`git submodule status` o buscar los crates `systems`/`a320_systems` bajo `core-rs/`)
   - Lee `docs/decisiones.md` (decisiones tomadas y pendientes, p. ej. PyO3 vs Rust-puro).

2. **Verifica qué funciona de verdad** (no solo que el directorio exista):
   - Fase 0: ¿hay un `main.rs`/ejemplo que instancie el avión, avance 1 s y lea una var eléctrica? ¿`cargo check` pasa en `core-rs/`?
   - Fase 1: ¿existe la API (`set`/`get`/`step`) y un REPL usable? ¿Funciona el escenario cold & dark → batería ON → buses con energía?
   - Fase 2: ¿`inject_failure` + `read_ecam` operativos? ¿La demo del generador caído levanta su caution?
   - Fase 3: ¿el servidor MCP arranca y expone los 9 tools?
   - Fase 4: ¿hidráulico/APU/fuel/arranque de motores con N2 de entrada?
   - Fase 5: ¿escenarios con ground truth y runner de scoring en `scenarios/`?

3. **Informa**:
   - Fase actual y evidencia (qué comprobaste, no suposiciones).
   - Checklist de lo que falta para cerrar la fase actual, con el criterio de éxito de `CLAUDE.md`.
   - Decisiones pendientes que bloquean (de `docs/decisiones.md`).
   - Si nada existe aún: el proyecto está pre-Fase 0; el siguiente paso es el spike de viabilidad.
