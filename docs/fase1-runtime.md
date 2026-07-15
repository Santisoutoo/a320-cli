# Fase 1 — Nota de diseño: runtime persistente sobre `Simulation<A320>`

*(Escrita al cierre de la exploración de Fase 0. El spike de Fase 0 usa `SimulationTestBed` por rapidez; esta nota fija el camino para el runtime real.)*

## Por qué no quedarse en `SimulationTestBed`

El test bed público de FBW (`systems/src/simulation/test.rs`) sirve para el spike, pero está pensado para tests cortos:

- Hardcodea `simulation_time = 100.` en `run_with_delta` (`test.rs:296`) — un runtime persistente necesita tiempo de simulación real y monótono.
- Sus internals (`TestReaderWriter`, `TestVariableRegistry`) son privados: no podemos inspeccionar ni volcar el registro completo de variables desde fuera, algo que `snapshot()` y `list_variables()` exigen.

## Diseño: envolver `Simulation<A320>` directamente

`pub struct Simulation<T: Aircraft>` (`systems/src/simulation/mod.rs:359`) es usable standalone y da control total por tick:

- `Simulation::new(start_state, A320::new, &mut registry)` — instancia el avión.
- `tick(delta, simulation_time, &mut reader_writer)` — un tick con `delta` y tiempo controlados por nosotros.
- `update_active_failures(FxHashSet<FailureType>)` — inyección/limpieza de fallos.

Para ello implementamos dos traits triviales (ambos `pub`, `mod.rs:30-40`):

| Trait | Contrato | Nuestra implementación |
|---|---|---|
| `VariableRegistry` | `get(String) -> VariableIdentifier` | `HashMap<String, VariableIdentifier>` propio — nos da `list_variables()` y lectura/escritura por nombre gratis |
| `SimulatorReaderWriter` | `read(&VariableIdentifier) -> f64` / `write(&VariableIdentifier, f64)` | `HashMap<VariableIdentifier, f64>` — el backing store de todas las vars; `snapshot()` es un volcado de este mapa |

El patrón a copiar es exactamente el de los privados `TestVariableRegistry` / `TestReaderWriter` de FBW (`systems/src/simulation/test.rs:618-686`, ~40 líneas). La diferencia: los nuestros son públicos, persistentes entre ticks y con el índice nombre→id conservado para el API (`set`/`get`/`list_*`).

Sobre esto, el tick loop del runtime es:

```
loop {
    aplicar escrituras pendientes (set de controles, set_environment)
    simulation.tick(delta, sim_time, &mut reader_writer)
    sim_time += delta
    servir lecturas (get / read_ecam / snapshot)
}
```

## El borde con el "mundo": simvars que consume `UpdateContext`

`UpdateContext` lo construye el framework (`UpdateContext::new_for_simulation`, y cada tick `update()` relee todo del reader — `update_context.rs:566-656`). Headless, **nosotros somos quienes escriben estas variables** en el backing store. Nombres exactos (constantes `*_KEY` en `update_context.rs:287-327`):

| Grupo | Simvars |
|---|---|
| Estado sim | `IS_READY`, `AIRCRAFT_PRESET_QUICK_MODE` |
| Velocidades | `AIRSPEED INDICATED`, `AIRSPEED TRUE`, `GPS GROUND SPEED`, `AIRSPEED MACH`, `VELOCITY WORLD Y`, `VELOCITY BODY X/Y/Z` |
| Posición/actitud | `PRESSURE ALTITUDE`, `PLANE ALT ABOVE GROUND`, `PLANE LATITUDE`, `PLANE PITCH DEGREES`, `PLANE BANK DEGREES`, `PLANE HEADING DEGREES TRUE` |
| Ambiente | `AMBIENT PRESSURE` (inHg), `AMBIENT TEMPERATURE`, `AMBIENT DENSITY`, `AMBIENT WIND X/Y/Z`, `AMBIENT PRECIP RATE`, `AMBIENT IN CLOUD`, `SURFACE TYPE` |
| Tierra | `SIM ON GROUND` |
| Aceleraciones | `ACCELERATION BODY X`, `ACCELERATION BODY Y`, `ACCELERATION_BODY_Z_WITH_REVERSER`, `ROTATION ACCELERATION BODY X/Y/Z`, `ROTATION VELOCITY BODY X/Y/Z` |
| Masas | `TOTAL WEIGHT`, `TOTAL WEIGHT YAW MOI`, `TOTAL WEIGHT PITCH MOI` |

`set_environment(alt, ias, oat, qnh, ...)` del contrato de la API es, en la práctica, un mapeo de parámetros de alto nivel a escrituras coherentes de este grupo de simvars (p. ej. en tierra: `SIM ON GROUND=1`, IAS=0, `PLANE ALT ABOVE GROUND`≈0, presión/temperatura del campo). Los defaults razonables para cold & dark en tierra quedan encapsulados en el runtime, no en el usuario.

## Alcance Fase 1 (recordatorio)

Vertical slice eléctrico en tierra: harness persistente + API (`set`/`get`/`step`) + REPL. Escenario objetivo: cold & dark → batería ON → ext pwr / APU gen → ver los buses cobrar vida en un `watch`. Sin failures todavía (Fase 2).
