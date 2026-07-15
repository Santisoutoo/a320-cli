# A320 Systems Twin — CLI + MCP para agentes LLM

## Qué es

Un **simulador headless de los sistemas del A320** (sin MSFS ni X-Plane corriendo) construido sobre el código open-source de FlyByWire, expuesto de dos formas:

1. **CLI** para que un humano opere el avión desde terminal (set switches, leer estado, avanzar tiempo, inyectar fallos).
2. **Servidor MCP** para que un **LLM opere el avión en bucle cerrado**: observa (ECAM + estado) → decide → actúa → avanza → observa.

Objetivo final: un entorno reproducible para **detección y gestión de fallos** siguiendo procedimientos reales (ECAM/QRH), pensado para servir de **benchmark de agentes** (research / paper). La contribución de investigación NO es el modelo del avión (es de FBW), sino el entorno evaluable + la suite de escenarios + el scoring de cumplimiento de procedimiento.

## Por qué este diseño (decisiones ya tomadas)

- El crate de sistemas de FBW (`systems` + `a320_systems`, en **Rust**) **no depende de MSFS en runtime**. Su propia suite de tests lo corre headless: instancia el avión, mete inputs, avanza el tiempo, lee outputs. Compilamos eso como binario nativo (no la build WASM) y lo ticamos desde nuestro proceso.
- La **interacción entre sistemas** (eléctrico ↔ hidráulico ↔ neumático ↔ motores) vive en `a320_systems`, no en el sim. MSFS solo aporta el "mundo exterior". Por eso corre sin simulador.
- **Failures**: FBW tiene un sistema de failures de primera clase (inyección por ID) y un **FWC/FWS** que genera los warnings de ECAM. Esas son las dos mitades: inyectar fallo → el FWC lo detecta → el agente lo gestiona.
- **Zibo (737) queda descartado**: no es open-source, no tiene motor de sistemas propio (es orquestación XLua dentro de X-Plane), no corre headless. El A320 de FBW es la única base viable para un engine headless.

## Qué reutilizas vs qué construyes (leer antes de empezar)

Esto NO es un `git clone && cargo run`. No existe un binario listo de un A320 headless con el que hablar. Aclaración de scope para no llevarse sorpresas en la Fase 0:

**Reutilizas (no lo reimplementas):** toda la *lógica de los sistemas* — presurización del circuito hidráulico, alimentación de buses eléctricos, secuencia de arranque del APU, lógica del FWC para levantar cautions, etc. Eso ya lo escribió FBW. No programas la física de los sistemas desde cero.

**Construyes (es la mayor parte del trabajo):**
1. **Decoupling de MSFS + build standalone** — sacar los crates de la infraestructura de `msfs-rs` y compilarlos nativos. Es la gran incógnita de la Fase 0. Se puede (su CI corre los tests sin sim), pero adaptar el harness de tests a un runtime *persistente e interactivo* no es trivial.
2. **Proveer el `UpdateContext` cada tick** — los inputs de "mundo" (ambiente, N2…). Sin esto los sistemas no funcionan.
3. **Gestión del registro de variables** — cablear cómo metes inputs y lees outputs de los ~cientos de vars que los sistemas leen/escriben.
4. **El loop de tick persistente, la API, la CLI, el MCP**, el mapeo de warnings del FWC a `read_ecam`, y (Fase 5) los escenarios de fallo con su ground truth.

**Analogía:** FBW te da **el bloque motor** (la lógica de sistemas, ya construida). Pero no hay coche alrededor — ni chasis, ni salpicadero, ni contacto. Tú construyes el chasis (harness headless), el salpicadero (CLI/MCP) y cableas el contacto (`UpdateContext` + registro de variables). El motor no se reconstruye; todo lo demás, sí.

**Nota para el paper:** esta infraestructura NO es trabajo "no-novel" desperdiciado. El *entorno evaluable headless* es parte de la contribución de investigación (la pieza de "infrastructure" del benchmark). Currárselo cuenta.

## Stack (baseline)

- **Core de simulación**: Rust, dependiendo de los crates `systems` + `a320_systems` vendorizados de `flybywiresim/aircraft` (GPLv3 — ver licencia abajo).
- **Binding**: **PyO3** para exponer el core a Python.
- **CLI + MCP server**: **Python** (SDK oficial de MCP), que es donde está la comodidad para orquestación de agentes.
- *Alternativa monolingüe*: todo en Rust con `rmcp` (sin FFI). Más simple de distribuir, menos ergonómico para la capa de agente. **Decisión a confirmar en Fase 0** (registrarla en `docs/decisiones.md`).

## Arquitectura (un core, dos frontends)

```
+-----------------------------+
|   Frontends                 |
|   - CLI (REPL, humano)      |
|   - MCP server (LLM)        |
+--------------+--------------+
               | (misma API)
+--------------v--------------+
|   Control/Observe API       |  <- capa limpia, sin CLI ni MCP
|   set / get / step / fail   |
|   read_ecam / snapshot      |
+--------------+--------------+
               | (PyO3)
+--------------v--------------+
|   Sim core (Rust)           |
|   - harness persistente     |
|   - tick loop               |
|   - registro de variables   |
|   - inyección de failures   |
|   - FBW systems + a320_systems (vendored) |
+-----------------------------+
```

Clave: construimos el **core + la API una vez**; CLI y MCP son dos ventanas a lo mismo.

## API interna (contrato)

- `set(control: str, value)` — actuar un switch/pulsador/knob (escribe una variable de entrada).
- `get(vars: list[str]) -> dict` — leer estado de sistemas.
- `read_ecam() -> list[Warning]` — warnings/cautions activos (capa de detección; probablemente mapeando LVARs de warning a mensajes).
- `step(dt_ms: int)` / `run(seconds, rate)` — avanzar la simulación.
- `set_environment(alt, ias, oat, qnh, ...)` — fijar el `UpdateContext` (mundo exterior).
- `inject_failure(id)` / `clear_failure(id)` / `list_failures() -> list[FailureId]`.
- `snapshot() -> dict` — volcado completo del estado.
- `list_controls()` / `list_variables()` — descubrimiento (para autocompletado en CLI y para el schema del MCP).

## Tools del MCP (lo que ve el LLM)

`set_control`, `read_state`, `read_ecam`, `advance`, `inject_failure`, `list_failures`, `clear_failure`, `snapshot`, `list_controls`.

Bucle del agente: `read_ecam` + `read_state` → razonar (QRH) → `set_control` → `advance` → repetir.

## Lo que hay que proveer (el borde con el "mundo")

Los sistemas necesitan un `UpdateContext`. Headless, esto lo damos nosotros:

- **En tierra (cold & dark / APU)**: casi nada — IAS=0, alt=elevación del campo, OAT/QNH fijos, motores off. Con esto ya interactúan eléctrico, hidráulico (bombas eléctricas + PTU), neumático, APU, fuel.
- **Con motores / en vuelo**: hay que alimentar **N1/N2** (el spool termodinámico lo calcula normalmente el sim) y condiciones ambiente. Opciones: fijar valores por régimen, un modelo de spool simplificado, o acoplar **JSBSim** para dinámica.

## Milestones

**Fase 0 — Spike de viabilidad (riesgo principal).**
Objetivo: compilar los crates `systems` + `a320_systems` de FBW como binario nativo standalone y hacer un `tick()` + leer una variable. Esto es lo más incierto (dependen de `msfs-rs` y del harness de simulación). Criterio de éxito: un `main.rs` que instancie el avión, avance 1s y lea al menos una var eléctrica. Decidir aquí Rust-puro vs Rust+PyO3.

**Fase 1 — Core + API + CLI, vertical slice eléctrico (en tierra).**
Harness persistente + API (`set`/`get`/`step`) + REPL. Escenario: cold & dark → batería ON → ext pwr / APU gen → ver los buses cobrar vida en un `watch`. Sin failures todavía.

**Fase 2 — Failures + detección.**
`inject_failure` / `list_failures` + `read_ecam` (mapear warnings del FWC). Demo: tirar un generador y ver la caution correspondiente aparecer.

**Fase 3 — MCP server.**
Exponer la API como tools MCP. Demo end-to-end: pasarle a un LLM "el APU gen ha caído, gestiónalo" y que actúe sobre los switches y resuelva.

**Fase 4 — Hidráulico + APU + fuel + arranque de motores.**
Ampliar subsistemas y el borde de "mundo" (N2 de entrada) para escenarios con motores.

**Fase 5 (research) — Suite de escenarios + scoring.**
Ground truth de procedimientos (failure → respuesta ECAM correcta → acciones QRH), métrica de cumplimiento a nivel de trayectoria, baselines con ≥2 modelos + ablations. Esta es la parte "paper".

## Constraints y gotchas (importante)

- **Licencia**: el código de FBW es **GPLv3**. Si vendorizamos sus crates, el proyecto hereda GPLv3. OK para personal/open, tenerlo presente.
- **Fijar versión de FBW** (pin a un commit/tag): actualizan casi cada semana; la reproducibilidad del benchmark lo exige.
- **No todo el avión es ese Rust**: el **FMS/flight-planning y la UI de la MCDU están en TypeScript**; el **FADEC fino del motor está en C++/WASM**. El núcleo de sistemas (elec/hyd/pneu/fuel/APU/presurización/tren/computers/FWC) sí es el Rust headless-able. Si el scope incluye MCDU, es otro subproyecto.
- **Lista de failures finita**: solo los que FBW haya implementado (rico igualmente: elec, hyd, fuel, engines, brakes, RA, ADIRS, computers…).
- **El harness de FBW es para tests cortos**, no un REPL vivo: hay que envolverlo en un loop persistente y stubbear los ~cientos de variables de entrada que los sistemas esperan. Ahí está la curra real, no en portar el modelo.

## Repo layout

```
/core-rs        # crate binario/lib: harness + API + FBW vendored (git submodule o subtree, pin)
/bindings       # PyO3 (si stack híbrido)
/cli            # REPL Python (rustyline si Rust puro)
/mcp            # servidor MCP Python
/scenarios      # (Fase 5) escenarios de fallo + ground truth de procedimientos
/docs
```

## Cómo trabajar en este repo

### Subagentes

| Subagente | Cuándo usarlo |
|---|---|
| `fbw-scout` | Localizar cualquier cosa dentro del código vendorizado de FBW: nombres de variables, failures disponibles, campos de `UpdateContext`, firma del harness de tests, lógica del FWC. Solo lectura, respuestas con rutas exactas. |
| `rust-core-dev` | Implementar/depurar el core Rust: decoupling de msfs-rs, harness persistente, tick loop, registro de variables, inyección de failures (Fases 0, 1, 2, 4). |
| `python-api-dev` | Bindings PyO3, CLI REPL y servidor MCP (Fases 1 y 3). |
| `scenario-engineer` | Escenarios de fallo, ground truth QRH y scoring del benchmark (Fase 5). |

Regla general: antes de bucear en el monorepo de FBW desde la conversación principal, delega la búsqueda en `fbw-scout` — el monorepo es enorme y contamina el contexto.

### Recordatorios operativos

- **Compilar siempre nativo.** Nunca el target WASM de FBW; si algo arrastra `msfs-rs` o wasm-bindgen al build nativo, eso es un bug de decoupling, no algo a instalar.
- **El submódulo/subtree de FBW va pineado** a un commit concreto. No actualizarlo sin registrar el cambio en `docs/decisiones.md` (la reproducibilidad del benchmark depende de ello).
- **GPLv3**: cualquier código que enlace con los crates de FBW hereda la licencia.
- **La Fase 0 es el riesgo principal.** Hasta que no haya un `main.rs` que instancie el avión, avance 1 s y lea una var eléctrica, no invertir en CLI/MCP.
- **Decisiones de arquitectura** (p. ej. Rust+PyO3 vs Rust-puro con `rmcp`) se documentan en `docs/decisiones.md` en cuanto se toman.

### Skills

- `/compilar-core` — compila y testea `core-rs/` filtrando el ruido del código vendorizado.
- `/estado-proyecto` — informa en qué fase está el proyecto y qué falta para cerrar la actual.
