# Plan de releases

Estrategia de versionado y publicación en GitHub (sección *Releases*). Objetivo: que cada release sea un punto **reproducible** del proyecto — especialmente de cara al benchmark y al paper, donde "evaluamos con a320-cli vX.Y.Z" tiene que significar algo exacto.

## Esquema de versionado

- **SemVer** (`vMAJOR.MINOR.PATCH`) con **una sola versión para todo el proyecto**: `core-rs`, `bindings`, `cli` y `mcp` se versionan en bloque (hoy los cuatro están en `0.1.0`). Mientras estemos en `0.x`, cada MINOR puede romper la API.
- El tag es `vX.Y.Z` sobre `main`, y la release de GitHub se crea sobre ese tag.
- **PATCH** solo para bugfixes sobre una release ya publicada.

## Roadmap de releases

| Release | Contenido | Estado de fases | Criterio de publicación |
|---|---|---|---|
| **v0.1.0** | Core headless + API + CLI + failures/ECAM + servidor MCP + arranque de motores. | Fases 0–4 (ya cerradas) | Tag sobre el `main` actual. Es la foto de "el simulador funciona end-to-end". |
| **v0.2.0** | TUI cockpit (Fase T, `feat/tui-poc`) si se mergea; mejoras de la API/MCP que surjan al usarla. | Fase T | El TUI opera un cold & dark → engines running completo. |
| **v0.3.0** | Suite de escenarios + ground truth QRH + scoring (versión alpha del benchmark). | Fase 5 (parte 1) | ≥N escenarios con ground truth y el scorer corriendo end-to-end sobre al menos 1 modelo. |
| **v1.0.0** | **Benchmark congelado para el paper**: suite final, scoring estable, baselines con ≥2 modelos + ablations ejecutadas. | Fase 5 (cierre) | Es la versión citable. A partir de aquí, la suite y el scoring no cambian sin subir MAJOR. |

Entre medias, `v0.x.y` de patch según haga falta. Si aparece trabajo grande no previsto (p. ej. acoplar JSBSim), se inserta como MINOR adicional y se corre la tabla.

## Qué lleva cada release de GitHub

1. **Tag anotado** `vX.Y.Z` en `main`.
2. **Notas de release** (en inglés, como los PRs) con:
   - Resumen de lo nuevo por subsistema (core / cli / mcp).
   - **Commit del pin del vendor de FBW** (submódulo `core-rs/vendor/aircraft`) — obligatorio: la reproducibilidad depende de él.
   - Cambios de API que rompen (si los hay).
3. **Sin binarios adjuntos de momento**: el build requiere el submódulo y el código hereda GPLv3; distribuir fuente + instrucciones es suficiente. Reevaluar wheels/binaries en v1.0.0 si facilita a terceros correr el benchmark.

## Proceso de publicación (checklist)

1. `main` verde en CI y fases del release cerradas en `docs/`.
2. Subir la versión en los 4 manifests (`core-rs/Cargo.toml`, `bindings/Cargo.toml`, `cli/pyproject.toml`, `mcp/pyproject.toml`) en un PR propio (`chore: release vX.Y.Z`).
3. Verificar con los comandos exactos del CI antes del tag.
4. Tras el merge: `git tag -a vX.Y.Z -m "..." && git push origin vX.Y.Z`.
5. `gh release create vX.Y.Z --title "..." --notes-file ...` con las notas del punto anterior.
6. Anotar en `docs/decisiones.md` solo si la release fija algo (p. ej. congelar la suite en v1.0.0).
