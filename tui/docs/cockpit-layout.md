<!-- Vendored from C:/Users/santi/Documents/a320/docs/cockpit-layout.md — synced 2026-07-21 -->

# A320 Cockpit — Layout ASCII para TUI

Esquema de paneles con box-drawing chars, alineado y listo para usar como mockup
(mismo orden vertical que el póster: overhead → glareshield → main panel → pedestal).

```text
                 ══ OVERHEAD · AFT ══
┌───────────────────────────────────────────────────────┐
│                CIRCUIT BREAKERS (C/B)                 │
├───────────────────────────────────────────────────────┤
│      MAINTENANCE · FADEC GND PWR · HYD LEAK VLVS      │
├───────────────────────────────────────────────────────┤
│              ELT · READING LT · DOME LT               │
└───────────────────────────────────────────────────────┘

                 ══ OVERHEAD · FWD ══
┌───────────┬───────────────────────────────┬───────────┐
│  F/CTL 1  │             ADIRS             │  F/CTL 2  │
├───────────┼───────────────────────────────┼───────────┤
│   EVAC    │   FIRE: ENG 1 · APU · ENG 2   │ CRG SMOKE │
├───────────┼───────────────────────────────┼───────────┤
│ EMER ELEC │              HYD              │ CRG HEAT  │
├───────────┼───────────────────────────────┼───────────┤
│   GPWS    │             FUEL              │   VENT    │
├───────────┼───────────────────────────────┼───────────┤
│ RCDR·CVR  │             ELEC              │ MAN START │
├───────────┼───────────────────────────────┼───────────┤
│  OXYGEN   │           AIR COND            │ RMP3·ACP3 │
├───────────┼───────────────────────────────┼───────────┤
│   CALLS   │ ANTI ICE · PROBE/WINDOW HEAT  │  INT LT   │
├───────────┼───────────────────────────────┼───────────┤
│   WIPER   │          CABIN PRESS          │   WIPER   │
├───────────┴───────────────────────────────┴───────────┤
│          APU (MASTER·START) · SIGNS · EXT LT          │
└───────────────────────────────────────────────────────┘

                              ══ GLARESHIELD ══
┌──────────┬────────────────┬──────────────────────────────┬────────────────┬──────────┐
│MSTR WARN │   EFIS CAPT    │             FCU              │    EFIS F/O    │MSTR WARN │
│MSTR CAUT │ QNH · FD · LS  │    SPD · HDG · ALT · V/S     │ QNH · FD · LS  │MSTR CAUT │
│  CHRONO  │ ND MODE · RNG  │ AP1 AP2 A/THR EXPED APPR LOC │ ND MODE · RNG  │ AUTOLAND │
└──────────┴────────────────┴──────────────────────────────┴────────────────┴──────────┘

                        ══ MAIN INSTRUMENT PANEL ══
┌─────────┬─────────┬────────┬───────────────┬───────────┬─────────┬─────────┐
│   PFD   │   ND    │  ISIS  │     E/WD      │ LDG GEAR  │   ND    │   PFD   │
│  CAPT   │  CAPT   │ CLOCK  │  (ECAM sup)   │   LEVER   │   F/O   │   F/O   │
│         │         │ DDRMI  │      SD       │ AUTO BRK  │         │         │
│         │         │        │  (ECAM inf)   │LO·MED·MAX │         │         │
├─────────┴─────────┴────────┴───────────────┴───────────┴─────────┴─────────┤
│                A/SKID & N/W STRG · BRAKE PRESS · PFD/ND XFR                │
└────────────────────────────────────────────────────────────────────────────┘

                    ══ PEDESTAL ══
┌───────────────────────────┬───────────────────────────┐
│          MCDU 1           │          MCDU 2           │
├──────────┬────────────────┴────────────────┬──────────┤
│  RMP 1   │            WX RADAR             │  RMP 2   │
├──────────┼─────────────────────────────────┼──────────┤
│  ACP 1   │SWITCHING (ATT HDG·AIR DATA·EIS) │  ACP 2   │
├──────────┴─────────────────────────────────┴──────────┤
│ ECAM CP: TO CFG · EMER CANC · SYS PAGES · CLR·STS·ALL │
├──────────┬─────────────────────────────────┬──────────┤
│ SPD BRK  │          THRUST LEVERS          │  FLAPS   │
│ (lever)  │ENG MASTER 1·2 / IGN-START-CRANK │ (0-FULL) │
├──────────┴─────────────────────────────────┴──────────┤
│            ATC/TCAS · RUDDER TRIM (+RESET)            │
├───────────────────────────────────────────────────────┤
│     PARK BRK · COCKPIT DOOR · FLOOD LT · PRINTER      │
└───────────────────────────────────────────────────────┘
 (TRIM WHEEL)                               (TRIM WHEEL)

( SIDESTICK CAPT )                       ( SIDESTICK F/O )
```
