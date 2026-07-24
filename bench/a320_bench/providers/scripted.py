"""A scripted agent: a fixed list of turns, no LLM, no network.

This is what CI runs. It exists so the whole episode pipeline — setup,
injection, validity gate, MCP session, recording, budgets, success
evaluation — is exercised end-to-end without a provider key, and so a
scenario's ground truth can be smoke-tested by scripting the procedure
itself (a script that flies the procedure must pass; one that ignores it
must fail).
"""

from typing import Any

from a320_bench.providers.base import ProviderAdapter, ToolCall, ToolResult, Turn


class ScriptedAdapter(ProviderAdapter):
    """Plays back `script`: each item is one turn, a list of (tool, args).

    When the script runs out, it returns empty turns (which the runner treats
    as an agent that stopped without reporting — useful for testing that path
    on purpose).
    """

    def __init__(self, script: list[list[tuple[str, dict[str, Any]]]]):
        self._script = list(script)
        self._cursor = 0
        self._calls = 0
        self.info: dict[str, Any] = {"provider": "scripted", "model": "scripted"}

    def _turn(self) -> Turn:
        if self._cursor >= len(self._script):
            return Turn(text="(script exhausted)", tool_calls=(), stop_reason="end_of_script")
        calls = []
        for name, args in self._script[self._cursor]:
            self._calls += 1
            calls.append(ToolCall(id=f"scripted-{self._calls}", name=name, args=args))
        self._cursor += 1
        return Turn(text="", tool_calls=tuple(calls), stop_reason="tool_use")

    def start(self, *, instructions: str, tools: list[dict[str, Any]], user_message: str) -> Turn:
        return self._turn()

    def next(self, results: list[ToolResult], *, nudge: "str | None" = None) -> Turn:
        return self._turn()
