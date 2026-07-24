"""The provider contract: what the episode runner needs from an agent.

Deliberately tiny. The runner mediates every tool call itself (that is where
the trajectory comes from), so an adapter only has to do two things: open the
conversation and produce the next turn given the previous turn's tool results.
Adapters are synchronous: the episode runs single-threaded around an
`unsendable` Sim, and a blocking provider call is fine — there is nothing else
to serve while the agent thinks.
"""

from dataclasses import dataclass, field
from typing import Any, Protocol


@dataclass(frozen=True)
class ToolCall:
    """One tool invocation requested by the agent."""

    id: str
    name: str
    args: dict[str, Any]


@dataclass(frozen=True)
class ToolResult:
    """What the runner sends back for one executed ToolCall."""

    call: ToolCall
    content: str
    is_error: bool


@dataclass(frozen=True)
class Turn:
    """One agent turn: optional prose plus zero or more tool calls, in order."""

    text: str
    tool_calls: tuple[ToolCall, ...]
    stop_reason: str = ""
    raw: dict[str, Any] = field(default_factory=dict, compare=False, repr=False)


class ProviderAdapter(Protocol):
    """Drives one conversation with one agent. One instance per episode."""

    #: Recorded verbatim into the trajectory's meta record: at least
    #: {"provider": ..., "model": ...}; real adapters add sampling params and
    #: client library versions.
    info: dict[str, Any]

    def start(self, *, instructions: str, tools: list[dict[str, Any]], user_message: str) -> Turn:
        """Open the conversation and return the agent's first turn.

        `tools` carries the MCP tool schemas as dicts: name, description,
        inputSchema — the adapter maps them to its provider's tool format.
        """
        ...

    def next(self, results: list[ToolResult], *, nudge: "str | None" = None) -> Turn:
        """Feed back the executed tool results and return the next turn.

        `nudge` is a runner-injected user message (used once, when a turn
        arrives with no tool calls and no report_done: the agent is reminded
        to continue or declare itself done).
        """
        ...
