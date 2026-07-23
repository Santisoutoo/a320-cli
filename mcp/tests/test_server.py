"""End-to-end tests of the MCP server, driven over the real stdio protocol.

These are not unit tests of the tool functions: they spawn the server as a
subprocess and talk to it with the SDK's own client, so what is verified is what
an agent would actually see — the tool list, the schemas, and the results
crossing the wire. That is what lets this half of Phase 3 be green without an
LLM in the loop; the LLM demo is the other half (#17).

Runnable two ways:
  - directly, no dependencies:  python mcp/tests/test_server.py
  - under pytest:               pytest mcp/tests/
"""

import asyncio
import sys
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

# The nine tools of the contract (CLAUDE.md). Nothing more, nothing less.
EXPECTED_TOOLS = {
    "set_control",
    "read_state",
    "read_ecam",
    "advance",
    "inject_failure",
    "list_failures",
    "clear_failure",
    "snapshot",
    "list_controls",
}

SERVER = StdioServerParameters(
    command=sys.executable,
    args=["-m", "a320_mcp", "--start", "cold-dark"],
    cwd=str(Path(__file__).resolve().parents[2]),
)


def run(coro):
    """Drive one coroutine to completion (each test gets a fresh server)."""
    return asyncio.run(coro)


async def _session(fn, server=SERVER):
    async with stdio_client(server) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            return await fn(session)


def _text(result) -> str:
    return " ".join(c.text for c in result.content if getattr(c, "type", None) == "text")


# --- the tool surface -------------------------------------------------------
def test_server_exposes_exactly_the_nine_tools():
    """The server starts over stdio and exposes the contract's nine tools."""

    async def check(session):
        return {t.name for t in (await session.list_tools()).tools}

    names = run(_session(check))
    assert names == EXPECTED_TOOLS, f"tool surface drifted: {names ^ EXPECTED_TOOLS}"


def test_the_agent_cannot_see_the_ground_truth():
    """`active_failures` and `list_variables` are absent ON PURPOSE (D-016).

    The binding exposes both. Neither is a tool:

    - `active_failures()` would hand the agent the answer. The benchmark measures
      whether it can diagnose from the ECAM; a tool that says "elec.apu_gen.1 is
      broken" turns that into a reading comprehension test.
    - `list_variables()` is hundreds of names and would bury the context window.

    This test exists because both are one line away from being added by someone
    who sees them on the binding and assumes the omission was an oversight.
    """

    async def check(session):
        return {t.name for t in (await session.list_tools()).tools}

    names = run(_session(check))
    assert "active_failures" not in names, "exposing active_failures leaks the ground truth"
    assert "list_variables" not in names, "exposing list_variables floods the context window"


def test_benchmark_profile_withholds_failure_tools_and_adds_report_done():
    """The benchmark tool surface is the graded agent's surface (#68).

    `inject_failure`/`clear_failure` must not exist there: the injected failure
    is the exam, and an agent that can repair the fault (or break something
    else) is not flying the procedure. `report_done` is the explicit
    end-of-episode channel the runner watches for. Checked in-process because
    the benchmark server is built by the Phase 5 runner, not by the stdio
    entry point.
    """
    import a320_sim
    from a320_mcp.server import create_server

    server = create_server(a320_sim.Sim(), profile="benchmark")

    async def check():
        names = {t.name for t in await server.list_tools()}
        ack = await server.call_tool("report_done", {"diagnosis": "x", "actions_summary": "y"})
        return names, ack

    names, ack = run(check())

    expected = (EXPECTED_TOOLS - {"inject_failure", "clear_failure"}) | {"report_done"}
    assert names == expected, f"benchmark surface drifted: {names ^ expected}"
    assert "episode is over" in str(ack), ack


def test_interactive_profile_is_the_default_and_rejects_unknown_profiles():
    """`create_server` defaults to the stdio surface; a typo'd profile fails loudly."""
    import a320_sim
    from a320_mcp.server import create_server

    server = create_server(a320_sim.Sim())

    async def check():
        return {t.name for t in await server.list_tools()}

    names = run(check())
    assert names == EXPECTED_TOOLS, f"interactive surface drifted: {names ^ EXPECTED_TOOLS}"

    try:
        create_server(a320_sim.Sim(), profile="benchmrak")
    except ValueError as exc:
        assert "benchmrak" in str(exc)
    else:
        raise AssertionError("an unknown profile should raise ValueError")


def test_instructions_profiles_are_wired_to_the_profile():
    """Each profile gets its INSTRUCTIONS variant unless one is passed explicitly.

    The instructions are prompt engineering and a Phase 5 ablation axis: the
    benchmark variant must tell the agent about `report_done`, and an explicit
    `instructions=` must win over the profile default (that is how ablations
    swap the text without touching the tool surface).
    """
    import a320_sim
    from a320_mcp.server import INSTRUCTIONS, create_server

    interactive = create_server(a320_sim.Sim())
    benchmark = create_server(a320_sim.Sim(), profile="benchmark")
    overridden = create_server(a320_sim.Sim(), profile="benchmark", instructions="ablated")

    assert interactive.instructions == INSTRUCTIONS
    assert "report_done" in benchmark.instructions
    assert overridden.instructions == "ablated"


def test_schemas_carry_the_catalogs_as_enums():
    """The valid names are generated from the catalogs, not hand-written (D-017).

    If the dynamic Literal ever stops producing an enum, the schema silently
    degrades to a free-form string and the model starts inventing control names.
    Nothing else would fail — hence this test.
    """

    async def check(session):
        return {t.name: t.inputSchema for t in (await session.list_tools()).tools}

    schemas = run(_session(check))

    control = schemas["set_control"]["properties"]["control"]
    assert "enum" in control, f"set_control.control has no enum: {control}"
    assert "bat_1" in control["enum"]
    assert "apu_gen" in control["enum"]

    failure = schemas["inject_failure"]["properties"]["failure_id"]
    assert "enum" in failure, f"inject_failure.failure_id has no enum: {failure}"
    assert "elec.tr.1" in failure["enum"]
    assert "elec.apu_gen.1" in failure["enum"]


# --- the loop ---------------------------------------------------------------
def test_the_agent_loop_over_the_wire():
    """Inject -> advance -> the ECAM says so -> clear -> it retires.

    The Phase 2 loop, driven entirely through MCP tool calls. This is the same
    TR 1 case as the core's integration test, which makes it a check that the
    protocol layer preserves behaviour rather than a new claim about the aircraft.
    """

    async def check(session):
        # Cold & dark: no power, no ECAM. Bring the network up first.
        for control in ("bat_1", "bat_2", "bus_tie", "ext_pwr_avail", "ext_pwr"):
            await session.call_tool("set_control", {"control": control, "value": 1})
        await session.call_tool("advance", {"seconds": 3})

        healthy = await session.call_tool("read_ecam", {})

        await session.call_tool("inject_failure", {"failure_id": "elec.tr.1"})
        await session.call_tool("advance", {"seconds": 3})
        failed = await session.call_tool("read_ecam", {})

        await session.call_tool("clear_failure", {"failure_id": "elec.tr.1"})
        await session.call_tool("advance", {"seconds": 3})
        repaired = await session.call_tool("read_ecam", {})
        return _text(healthy), _text(failed), _text(repaired)

    healthy, failed, repaired = run(_session(check))

    assert "TR 1 FAULT" not in healthy, f"healthy network should be clear: {healthy}"
    assert "ELEC TR 1 FAULT" in failed, f"the failure should raise its caution: {failed}"
    assert "TR 1 FAULT" not in repaired, f"clearing should retire the caution: {repaired}"


def test_reading_state_and_advancing_time():
    """`advance` moves the clock and `read_state` reports the network."""

    async def check(session):
        await session.call_tool("set_control", {"control": "bat_1", "value": 1})
        await session.call_tool("set_control", {"control": "bat_2", "value": 1})
        advanced = await session.call_tool("advance", {"seconds": 3})
        state = await session.call_tool(
            "read_state", {"variables": ["ELEC_DC_BAT_BUS_IS_POWERED", "ELEC_AC_1_BUS_IS_POWERED"]}
        )
        return _text(advanced), state.structuredContent

    advanced, state = run(_session(check))
    assert "t=3.0s" in advanced, advanced
    assert state["ELEC_DC_BAT_BUS_IS_POWERED"] == 1.0, "batteries should power the DC BAT bus"
    assert state["ELEC_AC_1_BUS_IS_POWERED"] == 0.0, "no AC source yet"


# --- start states -----------------------------------------------------------
def test_engines_running_start_state_hands_over_a_healthy_aircraft():
    """`--start engines-running` delivers the Phase 4 closing state (#60).

    The harness runs the whole cold & dark -> engines running sequence before
    serving (the scenario is the setup, not the agent's task, D-017), so the
    agent's first observation is a healthy aircraft: both engines at idle, the
    AC network on the engine generators, all three hydraulic circuits
    pressurized, the APU shut down, and a clean ECAM. This mirrors the core's
    closing integration test (core-rs/tests/cold_dark_to_engines_running.rs)
    over the real protocol.
    """
    server = StdioServerParameters(
        command=sys.executable,
        args=["-m", "a320_mcp", "--start", "engines-running"],
        cwd=str(Path(__file__).resolve().parents[2]),
    )

    async def check(session):
        state = await session.call_tool(
            "read_state",
            {
                "variables": [
                    "ENGINE_STATE:1",
                    "ENGINE_STATE:2",
                    "ELEC_AC_1_BUS_IS_POWERED",
                    "ELEC_AC_2_BUS_IS_POWERED",
                    "HYD_GREEN_SYSTEM_1_SECTION_PRESSURE",
                    "HYD_YELLOW_SYSTEM_1_SECTION_PRESSURE",
                    "HYD_BLUE_SYSTEM_1_SECTION_PRESSURE",
                    "OVHD_APU_START_PB_IS_AVAILABLE",
                ]
            },
        )
        ecam = await session.call_tool("read_ecam", {})
        return state.structuredContent, ecam

    state, ecam = run(_session(check, server))

    assert state["ENGINE_STATE:1"] == 1.0, "engine 1 should be running (On)"
    assert state["ENGINE_STATE:2"] == 1.0, "engine 2 should be running (On)"
    assert state["ELEC_AC_1_BUS_IS_POWERED"] == 1.0, "AC 1 should be on GEN 1"
    assert state["ELEC_AC_2_BUS_IS_POWERED"] == 1.0, "AC 2 should be on GEN 2"
    for circuit in ("GREEN", "YELLOW", "BLUE"):
        psi = state[f"HYD_{circuit}_SYSTEM_1_SECTION_PRESSURE"]
        assert 2800 <= psi <= 3100, f"{circuit} should be at nominal pressure, got {psi}"
    assert state["OVHD_APU_START_PB_IS_AVAILABLE"] == 0.0, "the APU should be shut down"

    warnings = (ecam.structuredContent or {}).get("result", [])
    assert warnings == [], f"a healthy aircraft should have a clean ECAM: {warnings}"
    assert "FAULT" not in _text(ecam), _text(ecam)


# --- errors and bounds ------------------------------------------------------
def test_errors_come_back_usable_not_as_crashes():
    """A bad argument is an actionable message, and the server keeps serving."""

    async def check(session):
        unknown = await session.call_tool("read_state", {"variables": ["NO SUCH VAR"]})
        too_long = await session.call_tool("advance", {"seconds": 100000})
        # The session still works after both errors.
        alive = await session.call_tool("read_ecam", {})
        return unknown, too_long, alive

    unknown, too_long, alive = run(_session(check))

    assert unknown.isError, "an unknown variable should be an error"
    assert "unknown control" in _text(unknown), _text(unknown)
    assert too_long.isError, "advance beyond the cap should be an error"
    assert "at most" in _text(too_long), _text(too_long)
    assert not alive.isError, "the server should survive tool errors"


def test_snapshot_refuses_to_flood_the_context():
    """A filter that matches too much is rejected instead of dumped (bounded output)."""

    async def check(session):
        wide = await session.call_tool("snapshot", {"contains": "_"})
        narrow = await session.call_tool("snapshot", {"contains": "ELEC_AC_1_BUS"})
        missing = await session.call_tool("snapshot", {"contains": "NOTHING_MATCHES_THIS"})
        return wide, narrow, missing

    wide, narrow, missing = run(_session(check))

    assert wide.isError, "an over-broad filter should be refused, not dumped"
    assert "Narrow it" in _text(wide), _text(wide)
    assert not narrow.isError, _text(narrow)
    assert "ELEC_AC_1_BUS_IS_POWERED" in narrow.structuredContent
    assert missing.isError and "no variable name contains" in _text(missing)


if __name__ == "__main__":
    tests = sorted(
        (name, fn)
        for name, fn in globals().items()
        if name.startswith("test_") and callable(fn)
    )
    for name, fn in tests:
        fn()
        print(f"ok  {name}")
    print(f"\n{len(tests)} MCP server tests passed.")
