<!-- Vendored from C:/Users/santi/Documents/a320/docs/pedestal-layout.md — synced 2026-07-21 -->

# A320 Pedestal — Layout TUI fiel a la disposición real

Notación:
- `[X]` korry pushbutton · `«X»` pushbutton con guarda · `<X>` toggle/switch · `(X)` knob/selector

```text
┌───────── MCDU 1 ─────────┐ ┌─────────────── SWITCHING ────────────────┐ ┌───────── MCDU 2 ─────────┐
│   (DISPLAY 14 lineas)    │ │ (ATT HDG) (AIR DATA) (EIS DMC) (ECAM/ND) │ │   (DISPLAY 14 lineas)    │
│ [LSK 1L-6L]  [LSK 1R-6R] │ │     cada uno: CAPT 3 · NORM · F/O 3      │ │ [LSK 1L-6L]  [LSK 1R-6R] │
│ [DIR][PROG][PERF][INIT]  │ ├──────────────── ECAM CP ─────────────────┤ │ [DIR][PROG][PERF][INIT]  │
│[DATA][FPLN][RADNAV][FUEL]│ │   (UPPER BRT) [T.O CONFIG] (LOWER BRT)   │ │[DATA][FPLN][RADNAV][FUEL]│
│[SEC][ATC][MENU][AIRPORT] │ │   [ENG][BLEED][PRESS][ELEC][HYD][FUEL]   │ │[SEC][ATC][MENU][AIRPORT] │
│ (slew) [A-Z] [0-9] [CLR] │ │   [APU][COND][DOOR][WHEEL][F/CTL][ALL]   │ │ (slew) [A-Z] [0-9] [CLR] │
├───────── RMP 1 ──────────┤ │   [EMER CANC]  [CLR] [STS] [RCL] [CLR]   │ ├───────── RMP 2 ──────────┤
│ACT 118.000 ⇄ 118.000 STB │ ├──────────── THRUST QUADRANT ─────────────┤ │ACT 118.000 ⇄ 118.000 STB │
│[VHF1][VHF2][VHF3] (TUNE) │ │  (TRIM WHEEL) │THR1│THR2│ (TRIM WHEEL)   │ │[VHF1][VHF2][VHF3] (TUNE) │
│[HF1] [AM] [HF2] <ON·OFF> │ │detents: TOGA · FLX/MCT · CL · IDLE · REV │ │[HF1] [AM] [HF2] <ON·OFF> │
│  «NAV» [VOR][ILS][MLS]   │ │      [A/THR DISC]×2  <REV LATCH>×2       │ │  «NAV» [VOR][ILS][MLS]   │
│       [ADF] [BFO]        │ ├────────────────── ENG ───────────────────┤ │       [ADF] [BFO]        │
├───────── ACP 1 ──────────┤ │  FIRE·FAULT      (MODE)      FIRE·FAULT  │ ├───────── ACP 2 ──────────┤
│  TX: [VHF1·2·3] [HF1·2]  │ │  <MASTER 1>  CRANK·NORM·IGN  <MASTER 2>  │ │  TX: [VHF1·2·3] [HF1·2]  │
│     [INT] [CAB] [PA]     │ ├──────────────── RUD TRIM ────────────────┤ │     [INT] [CAB] [PA]     │
│(RX knobs) [VOICE][RESET] │ │  display [L 0.8] · (NOSE L·R) · [RESET]  │ │(RX knobs) [VOICE][RESET] │
├─────────── LT ───────────┤ ├────────────── PARKING BRK ───────────────┤ ├───── LT · AIDS/DFDR ─────┤
│ (FLOOD) (INTEG MAIN&PED) │ │          (PULL & TURN · ON/OFF)          │ │   (FLOOD) [AIDS PRINT]   │
├──────── WX RADAR ────────┤ ├──────────── GEAR GRVTY EXTN ─────────────┤ │       [DFDR EVENT]       │
│    <MULTISCAN> <GCS>     │ │         «PULL & TURN ×3» (roja)          │ ├─────── ATC / TCAS ───────┤
│   (MODE) (GAIN) (TILT)   │ ├──────────────── HANDSET ─────────────────┤ │ code: [4521] (keys 0-7)  │
│   (SYS: 1·OFF·2) <PWS>   │ │                (HANDSET)                 │ │   (MODE: STBY·AUTO·ON)   │
├────── SPEED BRAKE ───────┤ └──────────────────────────────────────────┘ │   <ALT RPTG> <SYS 1·2>   │
│ lever: RET · 1/2 · FULL  │                                              │         [IDENT]          │
│     (pull up = ARM)      │                                              │  (TCAS: STBY·TA·TA/RA)   │
├────── COCKPIT DOOR ──────┤                                              │ (TRAF: THRT·ALL·ABV·BLW) │
│  OPEN / FAULT (lights)   │                                              ├───────── FLAPS ──────────┤
│    <UNLOCK·NORM·LOCK>    │                                              │   lever: 0·1·2·3·FULL    │
├────── DATA LOADER ───────┤                                              ├──────── PRINTER ─────────┤
│         (access)         │                                              │       (paper slot)       │
└──────────────────────────┘                                              └──────────────────────────┘
```
