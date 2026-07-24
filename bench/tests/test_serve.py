"""``a320-bench serve`` over the real stdio protocol (#19).

Same pattern as mcp/tests/test_server.py: spawn the command as a subprocess
and talk to it with the SDK client — what is verified is what an external
agent (claude -p on a subscription) would actually see.
"""

import asyncio
import json
import sys
import tempfile
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

from a320_bench.scenario import REPO_ROOT

SCENARIO = REPO_ROOT / "scenarios" / "elec" / "apu_gen_fault.yaml"


def test_serve_hands_an_injected_aircraft_to_an_external_client():
    """The client sees the benchmark surface and the failed aircraft; the
    result JSON judges the run after the client hangs up."""
    with tempfile.TemporaryDirectory() as tmp:
        result_path = Path(tmp) / "result.json"
        server = StdioServerParameters(
            command=sys.executable,
            args=[
                "-m",
                "a320_bench.cli",
                "serve",
                "--scenario",
                str(SCENARIO),
                "--result",
                str(result_path),
            ],
            cwd=str(REPO_ROOT),
        )

        async def drive(session):
            tools = {t.name for t in (await session.list_tools()).tools}
            ecam = await session.call_tool("read_ecam", {})
            # Manage the failure the blunt way: alternate source on.
            await session.call_tool("set_control", {"control": "ext_pwr", "value": 1})
            await session.call_tool("advance", {"seconds": 5})
            done = await session.call_tool(
                "report_done", {"diagnosis": "apu gen down", "actions_summary": "ext pwr on"}
            )
            return tools, ecam, done

        async def check():
            async with stdio_client(server) as (read, write):
                async with ClientSession(read, write) as session:
                    await session.initialize()
                    return await drive(session)

        tools, ecam, done = asyncio.run(check())

        assert "report_done" in tools and "clear_failure" not in tools
        messages = [w["message"] for w in ecam.structuredContent["result"]]
        assert "APU GEN FAULT" in messages, messages
        assert not done.isError

        # The subprocess exited when the session closed; atexit wrote the verdict.
        result = json.loads(result_path.read_text(encoding="utf-8"))
        assert result["scenario"] == "elec-apu-gen-fault"
        assert result["success_eval"]["all_passed"] is True
        assert result["active_failures"] == ["elec.apu_gen.1"]


def test_serve_parser_and_run_still_parse():
    from a320_bench.cli import build_parser

    serve = build_parser().parse_args(["serve", "--scenario", "s.yaml", "--result", "r.json"])
    assert serve.command == "serve" and serve.result == "r.json"
    run = build_parser().parse_args(["run", "--scenario", "s.yaml", "--model", "m"])
    assert run.command == "run"


if __name__ == "__main__":
    tests = sorted(
        (name, fn) for name, fn in globals().items() if name.startswith("test_") and callable(fn)
    )
    for name, fn in tests:
        fn()
        print(f"ok  {name}")
    print(f"\n{len(tests)} serve tests passed.")
