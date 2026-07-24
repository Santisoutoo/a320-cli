"""Real LLM providers through litellm's OpenAI-format completion API.

Why litellm (decision in docs/decisiones.md): one adapter covers every
provider the baselines need, at the price of a translation layer — which is
why the version is pinned **exactly** in ``bench/pyproject.toml`` and recorded
in every trajectory's meta. Verified against litellm 1.93.0:
``completion(model, messages, ..., tools, tool_choice)`` and messages whose
``tool_calls[].function`` carry ``name`` + ``arguments`` (a JSON string).

This module is NOT imported by ``a320_bench.providers`` eagerly: CI runs
without the ``[providers]`` extra, so litellm must stay an opt-in import.
"""

import json
from importlib import metadata
from typing import Any

from a320_bench.providers.base import ProviderAdapter, ToolCall, ToolResult, Turn

try:
    import litellm
except ImportError as exc:  # pragma: no cover - environment guard
    raise ImportError(
        "litellm is not installed. The real-provider adapter needs the "
        "[providers] extra: pip install -e 'bench/[providers]'"
    ) from exc


def _mcp_tools_to_openai(tools: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """MCP tool schemas map 1:1 onto OpenAI function-calling tools."""
    return [
        {
            "type": "function",
            "function": {
                "name": t["name"],
                "description": t["description"],
                "parameters": t["inputSchema"],
            },
        }
        for t in tools
    ]


class LiteLLMAdapter(ProviderAdapter):
    """One conversation with one model through ``litellm.completion``.

    Blocking calls on purpose: the episode is single-threaded around an
    `unsendable` Sim and there is nothing to serve while the model thinks.
    `sampling` is passed through to completion verbatim and recorded in
    `info` — the harness does not choose sampling defaults, the experiment
    config does.
    """

    def __init__(self, model: str, *, sampling: "dict[str, Any] | None" = None):
        self.model = model
        self._sampling = dict(sampling or {})
        self._messages: list[dict[str, Any]] = []
        self._tools: list[dict[str, Any]] = []
        self.info: dict[str, Any] = {
            "provider": "litellm",
            "model": model,
            "sampling": self._sampling,
            "litellm_version": metadata.version("litellm"),
        }

    def start(self, *, instructions: str, tools: list[dict[str, Any]], user_message: str) -> Turn:
        self._tools = _mcp_tools_to_openai(tools)
        self._messages = [
            {"role": "system", "content": instructions},
            {"role": "user", "content": user_message},
        ]
        return self._complete()

    def next(self, results: list[ToolResult], *, nudge: "str | None" = None) -> Turn:
        for result in results:
            self._messages.append(
                {
                    "role": "tool",
                    "tool_call_id": result.call.id,
                    "content": result.content if not result.is_error
                    else f"ERROR: {result.content}",
                }
            )
        if nudge is not None:
            self._messages.append({"role": "user", "content": nudge})
        return self._complete()

    def _complete(self) -> Turn:
        response = litellm.completion(
            model=self.model,
            messages=self._messages,
            tools=self._tools,
            **self._sampling,
        )
        choice = response.choices[0]
        message = choice.message

        # The assistant message goes back into history in provider format so
        # the next completion sees its own tool calls. `tool_calls` is omitted
        # (not None) when there are none: the OpenAI passthrough sends the
        # message dict verbatim (litellm 1.93.0
        # llms/openai/chat/gpt_transformation.py:451-455), and OpenAI rejects
        # an explicit null; the Anthropic path reads it with `.get(...)`
        # (prompt_templates/factory.py:2539) so absent and None are equivalent.
        assistant_message: dict[str, Any] = {
            "role": "assistant",
            "content": message.content,
        }
        if message.tool_calls:
            assistant_message["tool_calls"] = [
                {
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.function.name,
                        "arguments": tc.function.arguments,
                    },
                }
                for tc in message.tool_calls
            ]
        self._messages.append(assistant_message)

        calls = []
        malformed: dict[str, str] = {}
        for tc in message.tool_calls or []:
            try:
                args = json.loads(tc.function.arguments) if tc.function.arguments else {}
            except json.JSONDecodeError:
                args = None
            if not isinstance(args, dict):
                # Malformed (or non-object) arguments become {}: for tools with
                # required params the schema rejects the call and the recorded
                # is_error stays the agent's mistake. The original payload is
                # preserved in `raw` so the trajectory keeps the evidence for
                # the scorer.
                malformed[tc.id] = tc.function.arguments
                args = {}
            calls.append(ToolCall(id=tc.id, name=tc.function.name, args=args))

        usage = getattr(response, "usage", None)
        raw: dict[str, Any] = {
            "finish_reason": choice.finish_reason,
            "usage": usage.model_dump() if hasattr(usage, "model_dump") else None,
        }
        if malformed:
            raw["malformed_tool_arguments"] = malformed
        return Turn(
            text=message.content or "",
            tool_calls=tuple(calls),
            stop_reason=choice.finish_reason or "",
            raw=raw,
        )
