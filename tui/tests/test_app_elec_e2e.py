"""End-to-end against the running app: the ELEC slice must survive the
switchover to the generated overhead, and local controls must never touch
the sim.

Driven through the real widget message path (``App.run_test``): pressing
a wired korry reaches ``SimBridge.set`` via ``KorryButton.Pressed``;
pressing a local widget reaches the ``CockpitController`` via
``Actuated``. Sim time advances through the bridge exactly like the tick
does (200 ms steps).
"""

from __future__ import annotations

import asyncio

from a320_tui.app import A320TuiApp
from a320_tui.widgets.guarded_button import GuardedButton
from a320_tui.widgets.korry_button import KorryButton


def _settle(app: A320TuiApp, seconds: float) -> None:
    for _ in range(int(seconds * 1000 / 200)):
        app.bridge.step(200)


def _wired_button(app: A320TuiApp, control: str) -> KorryButton:
    return next(b for b in app.query(KorryButton) if b.spec.control == control)


def _local_widget(app: A320TuiApp, widget_type, control_id: str):
    return next(
        w for w in app.query(widget_type) if w._view.id == control_id
    )


def test_the_elec_slice_still_works_on_the_generated_overhead():
    async def script() -> None:
        app = A320TuiApp()
        async with app.run_test(size=(160, 50)) as pilot:
            await pilot.pause()

            # Cold & dark: the whole network is dead.
            state = app.bridge.read_state()
            assert not state.is_on("ELEC_DC_BAT_BUS_IS_POWERED")

            # Batteries ON through the wired korrys' real message path.
            _wired_button(app, "bat_1")._press()
            _wired_button(app, "bat_2")._press()
            await pilot.pause()
            _settle(app, 5)
            state = app.bridge.read_state()
            assert state.is_on("ELEC_DC_BAT_BUS_IS_POWERED")
            assert not state.is_on("ELEC_AC_1_BUS_IS_POWERED")

            # GPU plugged (world/scenario), EXT PWR + BUS TIE -> AC alive.
            _wired_button(app, "ext_pwr_avail")._press()
            await pilot.pause()
            _settle(app, 1)
            _wired_button(app, "ext_pwr")._press()
            _wired_button(app, "bus_tie")._press()
            await pilot.pause()
            _settle(app, 5)
            state = app.bridge.read_state()
            assert state.is_on("ELEC_AC_1_BUS_IS_POWERED")
            assert state.is_on("ELEC_DC_1_BUS_IS_POWERED")

            # A failure raises its caution on the E/WD data; clearing heals it.
            app.repl.execute("fail elec.tr.1")
            _settle(app, 3)
            state = app.bridge.read_state()
            assert any("TR 1" in message for _, message, _ in state.ecam)
            app.repl.execute("unfail elec.tr.1")
            _settle(app, 3)
            assert app.bridge.read_state().ecam == ()

    asyncio.run(script())


def test_local_controls_actuate_without_touching_the_sim():
    async def script() -> None:
        app = A320TuiApp()
        async with app.run_test(size=(160, 50)) as pilot:
            await pilot.pause()

            sim_writes: list[tuple[str, float]] = []
            original_set = app.bridge.set
            app.bridge.set = lambda control, value: (
                sim_writes.append((control, value)),
                original_set(control, value),
            )

            # A guarded button takes two steps: open, then press.
            guarded = _local_widget(app, GuardedButton, "EMER_MAN_ON")
            guarded.key_enter()
            await pilot.pause()
            assert not app.controller.registry.state("EMER_MAN_ON").pressed
            guarded.key_enter()
            await pilot.pause()
            assert app.controller.registry.state("EMER_MAN_ON").pressed

            # ADIRS selector cycles OFF -> NAV with the bracket key.
            from a320_tui.widgets.rotary import RotarySelector

            ir1 = _local_widget(app, RotarySelector, "IR1_MODE")
            ir1.key_right_square_bracket()
            await pilot.pause()
            assert app.controller.registry.state("IR1_MODE").pos == "NAV"

            # None of it reached the sim.
            assert sim_writes == []

    asyncio.run(script())
