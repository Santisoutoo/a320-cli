"""MCP server over ``a320_sim.Sim``: the LLM's window onto the same core.

Design notes
------------
- **No simulation logic here.** Every tool is a 1:1 mapping onto the API
  (``set``/``get``/``run``/``read_ecam``/failures/discovery). This is the second
  frontend onto the core the CLI already drives; anything that had to be
  reimplemented here rather than reused would be evidence the API boundary is in
  the wrong place. Nothing did.
- **Tools are plain sync functions, and that is load-bearing.** FastMCP calls a
  sync tool *inline on the event loop thread* (``func_metadata.py``:
  ``if fn_is_async: return await fn(...)`` / ``else: return fn(...)`` — no
  ``anyio.to_thread``, no executor). That is exactly what ``Sim`` needs: the
  aircraft uses ``Rc``/``RefCell`` internally, so the binding is ``unsendable``
  and touching it from another thread raises ``RuntimeError`` (D-010). See the
  warning above ``advance`` before "fixing" the blocking call. Ver D-015.
- **Schemas come from the catalogs, not from a second hand-written list.** The
  valid control names and failure ids are baked into the tool schemas as enums,
  generated at import from ``list_controls()`` / ``list_failures()``, so the
  model cannot invent a name that does not exist (D-017).
- **Tool descriptions are the agent's only documentation of an aircraft it
  cannot see.** They are prompt engineering, not docstrings — and in Phase 5
  they are an ablation axis. Written accordingly.
"""

import argparse
import sys
from typing import Literal

try:
    import a320_sim
except ImportError as exc:  # pragma: no cover - install-time guard
    sys.stderr.write(
        "error: cannot import the 'a320_sim' extension.\n"
        "       Build the bindings first, in a virtualenv:\n"
        "           pip install -e bindings/\n"
        f"       (import error: {exc})\n"
    )
    raise SystemExit(1)

from mcp.server.fastmcp import FastMCP
from mcp.server.fastmcp.exceptions import ToolError
from mcp.types import ToolAnnotations

# --- the aircraft -----------------------------------------------------------
# One Sim per process. stdio means one client per process, and every tool call
# lands on the same (event loop) thread, which is what `unsendable` requires.
#
# It is built at import, not in main(), because the tool schemas below embed the
# catalogs as enums and decorators run at import time. Cost: ~1 s to instantiate
# the A320. It would be paid at startup regardless.
_sim = a320_sim.Sim()

_CONTROLS = _sim.list_controls()
_FAILURES = _sim.list_failures()

# The schema enums. Only *friendly* catalog names are offered — not raw LVARs.
# That is the curated half of discovery doing its job (D-009): the agent actuates
# cockpit controls a human curated, and cannot reach an arbitrary variable. If a
# scenario needs a control that isn't here, the fix is to catalog it in
# core-rs/src/controls.rs, not to widen this enum.
ControlName = Literal[tuple(sorted(c["name"] for c in _CONTROLS))]  # type: ignore[valid-type]
FailureId = Literal[tuple(sorted(f["id"] for f in _FAILURES))]  # type: ignore[valid-type]

# Ceilings. Without them a single tool call can wedge the server or flood the
# context window — both of which look to the agent like the aircraft broke.
MAX_ADVANCE_S = 600.0
MAX_SNAPSHOT_VARS = 60

INSTRUCTIONS = """\
You operate the systems of an Airbus A320 in a headless simulator: electrical, \
hydraulic, pneumatic, fuel, APU and engines are all running, but there is no \
outside view and no instruments other than what these tools report.

The ECAM is your primary observation, exactly as it is for a real crew. \
`read_ecam` gives the active warnings and cautions, worst first. Diagnose from \
it and from `read_state`; there is no tool that tells you what is broken.

The loop is: `read_ecam` + `read_state` -> reason (ECAM/QRH procedure) -> \
`set_control` -> `advance` -> observe again.

TIME DOES NOT PASS BY ITSELF. A control you set has no effect until you call \
`advance`. If you set something and immediately read the state back, you will \
see the state from before your action and conclude, wrongly, that it did \
nothing. Systems also need a moment to settle: after acting, `advance` a couple \
of seconds before judging the result.

Discovery: `list_controls` for what you can actuate, `list_failures` for what \
can be broken, and `snapshot` with a filter for the readable state variables \
(there is no tool that lists them all — the list is hundreds long).
"""

mcp = FastMCP("a320-systems", instructions=INSTRUCTIONS)

# Every tool below is closed-world: the simulator is self-contained, and saying
# so keeps a client from treating these as calls out to the internet.
_READ_ONLY = ToolAnnotations(readOnlyHint=True, openWorldHint=False)


# --- observation ------------------------------------------------------------
@mcp.tool(annotations=_READ_ONLY)
def read_ecam() -> list[dict[str, str]]:
    """Read the active ECAM warnings and cautions, most severe first.

    This is what a pilot sees and reasons from, and it is your main source of
    truth about what is wrong. Each entry has: `message` (the ECAM text, e.g.
    "APU GEN FAULT"), `severity` (warning > caution > advisory), `system`, `id`,
    and `source`.

    `source` says who computed the warning: `vendor_flag` means the aircraft
    model itself raised the fault; `derived` means it was inferred from the
    aircraft's state. Both are real; the distinction is recorded for honesty.

    An empty list means the ECAM is clear. It is also empty when the ECAM is
    not powered (cold and dark) — with no electrical power there is no display,
    just as on the real aircraft. So an empty ECAM on an unpowered aircraft is
    not evidence that nothing is wrong.
    """
    return _sim.read_ecam()


@mcp.tool(annotations=_READ_ONLY)
def read_state(variables: list[str]) -> dict[str, float]:
    """Read specific state variables by name, e.g. `ELEC_AC_1_BUS_IS_POWERED`.

    Takes a list and returns a name -> value map. Booleans come back as 1.0 or
    0.0. Ask only for what you need: this is the precise instrument, not a dump.

    Use `snapshot` with a filter to discover which variable names exist, and
    `list_controls` to see the variables you can write (each control lists its
    underlying `lvar`). An unknown name is an error naming the offender, so a
    typo fails loudly rather than reading as 0.
    """
    return _sim.get(variables)


@mcp.tool(annotations=_READ_ONLY)
def snapshot(contains: str) -> dict[str, float]:
    """Discover state variables whose name contains `contains` (case-sensitive).

    This is how you find out what is observable — there is deliberately no tool
    that lists every variable, because the registry runs to hundreds of names
    and would bury the useful ones.

    Filter by system prefix and narrow from there: `ELEC_AC` for the AC network,
    `ELEC_DC` for DC, `OVHD_ELEC` for the electrical overhead panel, `APU` for
    the APU. A filter that matches too much is rejected — narrow it rather than
    reading everything.
    """
    matches = {k: v for k, v in _sim.snapshot().items() if contains in k}
    if not matches:
        raise ToolError(
            f"no variable name contains '{contains}'. Try a broader or different "
            f"filter (e.g. 'ELEC_AC', 'ELEC_DC', 'OVHD_ELEC', 'APU')."
        )
    if len(matches) > MAX_SNAPSHOT_VARS:
        raise ToolError(
            f"filter '{contains}' matches {len(matches)} variables (max "
            f"{MAX_SNAPSHOT_VARS}). Narrow it — e.g. '{contains}_' or a more "
            f"specific prefix — or read the ones you need with read_state."
        )
    return matches


# --- discovery --------------------------------------------------------------
@mcp.tool(annotations=_READ_ONLY)
def list_controls() -> list[dict[str, str]]:
    """List the cockpit controls you can actuate with `set_control`.

    Curated by hand, not a dump of the variable registry. Each entry has: `name`
    (what you pass to `set_control`), `lvar` (the underlying variable, readable
    with `read_state`), `kind`, `valid_values`, `description`, `group`, and
    `domain`.

    `domain` is worth reading: `cockpit` is a real control a pilot actuates;
    `world` is outside state that a real simulator would provide and that we
    fake here (e.g. whether a ground power unit is plugged in).
    """
    return _CONTROLS


@mcp.tool(annotations=_READ_ONLY)
def list_failures() -> list[dict[str, str]]:
    """List the failures that can be injected with `inject_failure`.

    Each entry has a stable `id` (e.g. `elec.tr.1`), a `description`, a `group`,
    and `ata` (the ATA chapter id the aircraft vendor uses for the same failure).

    This is the catalog of what *can* break — not what *is* broken. To find out
    what is currently wrong, read the ECAM.
    """
    return _FAILURES


# --- action -----------------------------------------------------------------
@mcp.tool(
    annotations=ToolAnnotations(
        readOnlyHint=False,
        destructiveHint=False,  # additive: it sets a switch, it destroys nothing
        idempotentHint=True,  # writing the same value twice is the same state
        openWorldHint=False,
    )
)
def set_control(control: ControlName, value: float) -> str:
    """Actuate a cockpit control: flip a switch or push a pushbutton.

    `control` is a name from `list_controls`; `value` is 1 for on/auto and 0 for
    off (see each control's `valid_values`). An out-of-range value is rejected
    rather than silently coerced.

    The write lands immediately, but the aircraft does not react until time
    passes: call `advance` afterwards, then read the state back to confirm.
    """
    _sim.set(control, value)
    return f"{control} <- {value:g} (call advance() for the aircraft to react)"


@mcp.tool(
    annotations=ToolAnnotations(
        readOnlyHint=False,
        destructiveHint=False,
        idempotentHint=False,  # time moves: the same call twice is not the same
        openWorldHint=False,
    )
)
def advance(seconds: float, rate: float = 5.0) -> str:
    """Advance simulated time. Nothing you do takes effect until you call this.

    `seconds` is simulated time, not wall-clock: it runs as fast as it computes.
    `rate` is ticks per second (5 is the usual settling rate; leave it alone
    unless you have a reason).

    Rules of thumb: 2 seconds to let a contactor sequence settle after acting;
    5 seconds for a network to reconfigure after a failure; ~65 seconds for an
    APU to spin up. Returns the new simulated clock.
    """
    if seconds <= 0:
        raise ToolError(f"seconds must be positive, got {seconds}")
    if seconds > MAX_ADVANCE_S:
        raise ToolError(
            f"seconds must be at most {MAX_ADVANCE_S:g} in one call, got {seconds:g}. "
            f"Advance in steps and observe in between — that is the loop."
        )
    if rate <= 0:
        raise ToolError(f"rate must be positive, got {rate}")
    # Blocking on purpose. This runs on the event loop thread, and it must: the
    # Sim is `unsendable` (D-010), so handing this to anyio.to_thread to "avoid
    # blocking" would raise RuntimeError from the binding. With stdio there is a
    # single client and nothing else to serve, so there is nothing to block.
    _sim.run(seconds, rate)
    return f"advanced {seconds:g}s (t={_sim.sim_time():.1f}s)"


@mcp.tool(
    annotations=ToolAnnotations(
        readOnlyHint=False,
        destructiveHint=True,  # it breaks a system (reversibly, via clear_failure)
        idempotentHint=True,  # a set: injecting twice is injecting once
        openWorldHint=False,
    )
)
def inject_failure(failure_id: FailureId) -> str:
    """Break something: inject a failure by its id from `list_failures`.

    Reversible with `clear_failure`. Takes effect on the next `advance`.

    This is a scenario-authoring tool. If you are being asked to *manage* a
    failure, it has already been injected for you — diagnose it from the ECAM
    rather than injecting your own.
    """
    _sim.inject_failure(failure_id)
    return f"injected {failure_id} (call advance() for it to take effect)"


@mcp.tool(
    annotations=ToolAnnotations(
        readOnlyHint=False,
        destructiveHint=False,  # restores: the opposite of destructive
        idempotentHint=True,
        openWorldHint=False,
    )
)
def clear_failure(failure_id: FailureId) -> str:
    """Repair an injected failure by its id. Takes effect on the next `advance`.

    Clearing a failure that is not active is fine, not an error.

    Note this repairs the underlying fault — it is not how a crew responds to a
    failure. Managing one means reconfiguring the aircraft with `set_control`.
    """
    _sim.clear_failure(failure_id)
    return f"cleared {failure_id} (call advance() for it to take effect)"


# --- start states -----------------------------------------------------------
def _start_apu_running(sim: "a320_sim.Sim") -> None:
    """Leave the aircraft with the APU running and feeding the AC network.

    The exact sequence from the Phase 2 integration test
    (``core-rs/tests/generator_caution.rs``). Deliberately **no external power**:
    the APU GEN fault condition requires the ext pwr contactor to be open.
    Batteries are not optional — the APU's start motor hangs off both battery
    contactors.

    This is harness work, not agent work: the scenario is the setup, and the
    agent's task is to manage what happens next.
    """
    sim.set("UNLIMITED FUEL", 1)  # the Rust never burns fuel; it only reads it
    sim.set("bat_1", 1)
    sim.set("bat_2", 1)
    sim.run(3.0, 5.0)

    # Catalog names since Phase 4 slice 2 (#56); raw LVARs no longer needed.
    sim.set("apu_master", 1)
    sim.run(1.0, 5.0)
    sim.set("apu_start", 1)

    # Bounded wait, not a blind sleep: the APS3200 reaches available at ~62 s.
    elapsed = 0
    while sim.get(["OVHD_APU_START_PB_IS_AVAILABLE"])["OVHD_APU_START_PB_IS_AVAILABLE"] == 0.0:
        sim.run(1.0, 10.0)
        elapsed += 1
        if elapsed >= 150:
            raise RuntimeError("the APU did not reach available in 150 s of simulation")

    sim.set("apu_gen", 1)
    sim.set("bus_tie", 1)
    sim.run(5.0, 5.0)


START_STATES = {
    "cold-dark": lambda sim: None,  # the default: Sim() is already cold & dark
    "apu-running": _start_apu_running,
}


def main(argv: "list[str] | None" = None) -> int:
    """Console-script / ``python -m a320_mcp`` entry point."""
    parser = argparse.ArgumentParser(
        prog="a320-mcp",
        description="MCP server exposing the headless A320 systems core.",
    )
    parser.add_argument(
        "--start",
        choices=sorted(START_STATES),
        default="cold-dark",
        help=(
            "aircraft state to hand the agent (default: cold-dark). "
            "'apu-running' boots the APU and puts it on the AC network, which "
            "takes ~60 s of simulation at startup."
        ),
    )
    args = parser.parse_args(argv)

    # stderr, never stdout: stdout is the MCP transport and any stray print
    # there corrupts the protocol framing.
    print(f"a320-mcp: preparing start state '{args.start}'...", file=sys.stderr)
    START_STATES[args.start](_sim)
    print(f"a320-mcp: ready (t={_sim.sim_time():.1f}s), serving over stdio", file=sys.stderr)

    mcp.run()  # transport defaults to stdio
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
