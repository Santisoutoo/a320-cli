# Fase 4 — Nota de diseño: modelo de motor propio (slice 4, issue #58)

*(Hermana de `docs/fase1-runtime.md`. Decisiones asociadas: D-019 — spool de
primer orden — y D-020 — contrato de arranque. Rutas del vendor relativas a
`core-rs/vendor/aircraft`.)*

## Por qué el motor es nuestro

El Rust de FBW **no modela el motor**: el spool termodinámico vive en el FADEC
C++/WASM y en MSFS. Los sistemas Rust solo **leen** simvars de motor como
entrada pura, y nadie transiciona `ENGINE_STATE` en el Rust del vendor (solo su
test bed lo escribe en helpers de test). Headless, generar esos simvars es
nuestro trabajo — el mismo patrón que `Environment` con el `UpdateContext`,
pero con estado propio (máquina de estados + N2).

El modelo vive en `core-rs/src/engine.rs` (`EngineModel`, uno por motor). El
runtime lo tica **antes** de `simulation.tick`, de modo que el avión lee en el
mismo tick lo que el motor acaba de generar:

```
environment.write_all → engines.update(dt) → failures → simulation.tick
```

## Contrato: simvars generadas por tick y por motor

| Simvar | Unidad con que la lee el framework | Consumidor en el vendor |
|---|---|---|
| `ENGINE_N2:{n}` | percent (`Ratio`, `simulation/mod.rs:774`) | `LeapEngine` (uncorrected: `hydraulic_pump_output_speed`, `oil_pressure`, `is_above_minimum_idle` ≥ 55 %, `leap_engine.rs:36,61-69,100-107`) y el FADEC de pneumatic (`a320_systems/src/pneumatic.rs:1611-1612`) |
| `TURB ENG CORRECTED N2:{n}` | percent | `LeapEngine` (`leap_engine.rs:43`) |
| `TURB ENG CORRECTED N1:{n}` | percent | `LeapEngine` (`leap_engine.rs:42`; los packs del aire acondicionado consumen `EngineCorrectedN1`) |
| `TURB ENG JET THRUST:{n}` | **pound** (se lee como `Mass`, `simulation/mod.rs:781`; `leap_engine.rs:26,76`) | `LeapEngine::net_thrust` |
| `ENGINE_STATE:{n}` | enum sobre f64: Off=0 / On=1 / Starting=2 / Restarting=3 / Shutting=4 (`fbw-common/.../pneumatic/mod.rs:507-528`) | FADEC de pneumatic (`a320_systems/src/pneumatic.rs:1604-1605`) → válvula de arranque; aire acondicionado (`air_conditioning.rs:1220-1221`) |
| `GENERAL ENG STARTER ACTIVE:{n}` | bool | Controlador del PTU, que lo lee como **eng master on/off** (`a320_systems/src/hydraulic/mod.rs:3449-3452,3550-3554`) |

Y los inputs de cabina que el modelo lee del store:

| LVAR | De quién es | Valores |
|---|---|---|
| `ENG_MASTER_{n}` | **Nuestro** (D-020): en MSFS el engine master vive en el fuel system C++, no hay LVAR en el Rust del vendor | 0 = OFF, 1 = ON |
| `TURB ENG IGNITION SWITCH EX1:1` | Del vendor — un **único selector para ambos motores**, leído por el FADEC (`a320_systems/src/pneumatic.rs:1608-1609`) | `EngineModeSelector` (`fbw-common/.../pneumatic/mod.rs:764-782`): 0 = CRANK, 1 = NORM, 2 = IGN/START |

El runtime siembra una vez (`ENGINE_CONTROL_SEED`, `runtime.rs`): masters a 0 y
el selector a **1 (NORM)** — sin seed, una var no escrita lee 0.0 = CRANK, que
no es la posición de reposo del panel real.

## Máquina de estados

```
            master ON ∧ selector IGN/START
   Off ────────────────────────────────────► Starting
    ▲                                           │
    │ N2 < 1 %                        N2 ≥ 58 % │
    │                                           ▼
 Shutting ◄──────────────────────────────────── On
            master OFF (también aborta un
            arranque en curso desde Starting)
```

- Reutilizamos el enum `EngineState` del vendor: los valores escritos no pueden
  divergir de los que sus consumidores esperan. `Restarting` no se produce.
- Devolver el selector a NORM con el arranque en curso **no** lo aborta (como
  el FADEC real una vez secuenciado); solo el master corta.
- `GENERAL ENG STARTER ACTIVE:{n}` **espeja el master**, no el corte del
  starter: su único lector Rust es el PTU, que lo trata como master on/off, y
  el propio test bed del vendor lo deja a 1 mientras el motor corre
  (`hydraulic/mod.rs:7145-7183`). El corte del starter neumático ya lo modela
  el vendor: `EngineStarterValveController` abre la válvula con
  `Starting|Restarting` y N2 < 65 % (`a320_systems/src/pneumatic.rs:458-473`),
  alimentado por nuestros `ENGINE_STATE`/`ENGINE_N2`.

## Spool de N2: primer orden por tramos (D-019)

Integración con la discretización **exacta** del primer orden — determinista en
función del `dt` del tick, sin reloj de pared ni azar (requisito del
benchmark):

```
n2 += (target - n2) · (1 - e^(-dt/τ))
```

| Tramo | Condición | Target | τ | Timing resultante |
|---|---|---|---|---|
| Motoring (starter) | `Starting`, N2 < 25 % | 30 % | 8 s | light-off (25 %) en `8·ln 6 ≈ 14.3 s` |
| Aceleración | `Starting`, N2 ≥ 25 % | 59 % | 10 s | 58 % en `10·ln 34 ≈ 35.3 s` |
| Ralentí | `On` | 58.5 % | 4 s | asentamiento fino tras el arranque |
| Spool-down | `Shutting` / `Off` | 0 % | 12 s | < 1 % en `12·ln 58.5 ≈ 49 s` |

**Arranque total: ~50 s** hasta `On` (los tests exigen 40-70 s). Los targets de
motoring y aceleración están un poco por encima de sus umbrales (25 %/58 %) a
propósito: un primer orden nunca alcanza su target, y apuntar exactamente al
umbral dejaría el arranque clavado en la asíntota.

Derivadas de N2 (lineales, solo régimen de idle en tierra):

- **N1** = `n2 · 18.5/58.5` (N1 de ralentí 18.5 %).
- **Empuje** = `n2 · 1000/58.5` libras (~1000 lb al ralentí, 0 parado).

## Simplificaciones (deliberadas, v1)

- **Corrected = uncorrected**: en tierra a ISA la corrección por θ/δ es ≈ 1;
  escribimos el mismo valor en ambas familias de simvars.
- **Sin FADEC fino**: no hay palanca de gases en este slice; el único régimen
  es el ralentí en tierra. N1/empuje son rectas desde N2.
- **Sin gate de bleed**: el arranque v1 solo exige master + IGN/START; no
  comprueba presión neumática en el starter (llega en el slice 5 como stretch).
- **`Restarting` y CRANK fuera de alcance**: el modo CRANK (motoring sin
  ignición) se acepta en el selector pero no mueve el motor.
- Umbrales que el vendor deriva de nuestro N2 y "salen gratis": presión de
  aceite > 18 psi al cruzar 25 % de N2 (`leap_engine.rs:67-68`),
  `is_above_minimum_idle` a 55 %, corte de la válvula de arranque a 65 %.
