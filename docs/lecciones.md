# Registro de errores graves (lecciones)

Una entrada por fallo grave cometido. El objetivo no es la autopsia sino la **regla que evita la reincidencia**: cada entrada termina en una regla accionable, y este archivo se consulta antes de tareas del mismo tipo.

**Qué cuenta como grave** (si dudas, regístralo): un CI rojo evitable, trabajo perdido o un PR/rama rotos, un bug que llegó a `main`, mover el pin del vendor sin registrarlo, una acción destructiva ejecutada sobre el objetivo equivocado, o cualquier fallo cuya limpieza costó más que la tarea original.

**Formato**: `L-NNN — título` con **Fecha**, **Qué pasó**, **Causa raíz** y **Regla**. La numeración es secuencial; si trabajan agentes en paralelo, asignarles el número libre al lanzarlos (misma trampa que con las D-NNN, ver L-004).

## Lecciones

### L-001 — Verificar con los comandos exactos del CI, y `fmt` como último paso
**Fecha**: 2026-07-16 (Fase 2)
**Qué pasó**: dos CI rojos seguidos (~5 min cada uno: el CI recompila el vendor de FBW entero), más el baile de stash/fix/push con trabajo de la issue siguiente ya en el árbol.
**Causa raíz**: verificación aproximada en vez de la del CI. (1) `cargo fmt` corrió a mitad del trabajo y luego hubo más ediciones en `main.rs`: el commit se fue sin formatear. (2) `cargo clippy` corrió **sin** `-D warnings` filtrando por `^error`, así que un `unused_mut` salió como warning local pero como error en CI.
**Regla**: justo antes de `git commit` y después de la última edición: `cargo fmt && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`. Los comandos **exactos** de `.github/workflows/ci.yml`, no una versión parecida; filtrar salida sin `-D warnings` da una falsa sensación de verde.

### L-002 — Mergear el PR base de un stack con `--delete-branch` cierra los PRs hijos
**Fecha**: 2026-07-15 (Fase 1, ejecución paralelizada)
**Qué pasó**: al mergear el PR base de un stack con `--delete-branch`, GitHub **cerró** (no retargeteó) el PR hijo; un PR cerrado cuya rama base ya no existe no admite ni reapertura ni cambio de base, y hubo que recrearlo.
**Causa raíz**: asumir que GitHub retargetea los PRs hijos al desaparecer su rama base.
**Regla**: en PRs stacked, retargetear los PRs hijos a `dev` **antes** de borrar ramas del stack.

### L-003 — Verificar el trabajo de un agente repitiendo su mismo orden no caza bugs
**Fecha**: 2026-07-15 (Fase 1, issue #39)
**Qué pasó**: el wedge del primer tick (BatteryChargeLimiter latcheado en `Open`) pasó la verificación del agente y solo apareció después, al ejecutar `set bat_1 1` como **primer** comando del REPL — un orden que el agente nunca probó.
**Causa raíz**: verificar el deliverable reproduciendo la misma secuencia que usó quien lo implementó; los bugs de estado/orden quedan fuera por construcción.
**Regla**: al verificar el trabajo de un agente, conducir el deliverable con un orden **distinto** al que usó el agente (otros comandos primero, otro camino al mismo estado).

### L-004 — Agentes paralelos colisionan al numerar registros compartidos
**Fecha**: 2026-07-15 (Fase 1, Ola 2)
**Qué pasó**: dos agentes paralelos registraron cada uno una "D-009" en `docs/decisiones.md`; hubo que renumerar al mergear.
**Causa raíz**: el número siguiente se decide leyendo el archivo, y dos agentes concurrentes leen el mismo estado.
**Regla**: al lanzar agentes en paralelo que puedan escribir en un registro numerado (`decisiones.md`, este archivo), asignarles el número libre desde la conversación principal, o declarar en el prompt quién numera.

### L-005 — Afirmar cómo se comporta una dependencia por dentro sin leer su código
**Fecha**: 2026-07-17 (Fase 3, planificación de #17)
**Qué pasó**: al plantear la Fase 3 se presentó como trampa técnica que FastMCP ejecutaría los tools **síncronos en un hilo** (`anyio.to_thread`) y que eso chocaría con el `unsendable` del binding (D-010), recomendando escribirlos `async def` para evitarlo. Al leer el SDK, lo cierto es lo contrario: `func_metadata.py` los llama **inline en el hilo del event loop** (`if fn_is_async: return await fn(...)` / `else: return fn(...)`), sin `to_thread` ni executor. La recomendación era innecesaria y la trampa real es la **inversa**: el peligro es que alguien *añada* `to_thread` para no bloquear el event loop. Se detectó antes de escribir código, pero la afirmación ya había ido a una recomendación sobre la que se decidió el siguiente paso.
**Causa raíz**: afirmar de memoria el comportamiento interno de una dependencia que no se había leído, y presentarlo con el **mismo nivel de confianza** que los hallazgos sí verificados. El proyecto ya tiene la norma de citar `archivo:línea` del vendor de FBW para cualquier afirmación sobre su lógica (D-005, D-012, D-014 son eso); esa norma no se aplicó a una dependencia externa.
**Regla**: antes de escribir en un plan, una decisión o un doc cómo se comporta una dependencia por dentro (threading, defaults, firmas, versiones), **leer su código o su doc de la versión que vamos a fijar y citar `archivo:línea`** — el mismo estándar que se le exige al vendor. Lo no verificado se marca como "a verificar", no se afirma. Aplica igual a la versión: comprobar en PyPI/upstream cuál es la estable antes de asumir una API (la 2.0 de `mcp` ya usa otra).
