"""LiteLLMAdapter mapping tests with a mocked litellm.completion (#71).

No network, no keys: what is under test is the translation — MCP tool schemas
to OpenAI-format tools, provider tool calls to the runner's ToolCall, tool
results and the nudge back into the message history. Skipped entirely when
litellm is not installed (CI runs without the [providers] extra).
"""

from types import SimpleNamespace

import pytest

litellm = pytest.importorskip("litellm", reason="needs the [providers] extra")

from a320_bench.providers.base import ToolCall, ToolResult  # noqa: E402
from a320_bench.providers.litellm_adapter import LiteLLMAdapter  # noqa: E402

MCP_TOOLS = [
    {
        "name": "advance",
        "description": "Advance simulated time.",
        "inputSchema": {"type": "object", "properties": {"seconds": {"type": "number"}}},
    }
]


def _response(*, content=None, tool_calls=None, finish_reason="tool_use"):
    return SimpleNamespace(
        choices=[
            SimpleNamespace(
                message=SimpleNamespace(content=content, tool_calls=tool_calls),
                finish_reason=finish_reason,
            )
        ],
        usage=None,
    )


def _tool_call(id_, name, arguments):
    return SimpleNamespace(id=id_, function=SimpleNamespace(name=name, arguments=arguments))


@pytest.fixture
def captured(monkeypatch):
    """Mock litellm.completion, capturing every kwargs it was called with."""
    calls = []
    responses = []

    def fake_completion(**kwargs):
        calls.append(kwargs)
        return responses.pop(0)

    monkeypatch.setattr(litellm, "completion", fake_completion)
    return calls, responses


def test_start_maps_schemas_and_messages(captured):
    calls, responses = captured
    responses.append(
        _response(tool_calls=[_tool_call("c1", "advance", '{"seconds": 5}')])
    )

    adapter = LiteLLMAdapter("some/model")
    turn = adapter.start(
        instructions="SYSTEM TEXT", tools=MCP_TOOLS, user_message="TASK"
    )

    kwargs = calls[0]
    assert kwargs["model"] == "some/model"
    assert kwargs["messages"][0] == {"role": "system", "content": "SYSTEM TEXT"}
    assert kwargs["messages"][1] == {"role": "user", "content": "TASK"}
    tool = kwargs["tools"][0]
    assert tool["type"] == "function"
    assert tool["function"]["name"] == "advance"
    assert tool["function"]["parameters"] == MCP_TOOLS[0]["inputSchema"]

    assert turn.tool_calls == (ToolCall(id="c1", name="advance", args={"seconds": 5}),)
    assert turn.stop_reason == "tool_use"


def test_next_feeds_results_and_nudge_into_history(captured):
    calls, responses = captured
    responses.append(_response(tool_calls=[_tool_call("c1", "advance", "{}")]))
    responses.append(_response(content="done", tool_calls=None, finish_reason="stop"))

    adapter = LiteLLMAdapter("some/model")
    turn1 = adapter.start(instructions="S", tools=MCP_TOOLS, user_message="U")
    adapter.next(
        [ToolResult(call=turn1.tool_calls[0], content="t=5.0s", is_error=False)],
        nudge="NUDGE TEXT",
    )

    history = calls[1]["messages"]
    # assistant turn with its tool call went back in provider format
    assistant = history[2]
    assert assistant["role"] == "assistant"
    assert assistant["tool_calls"][0]["function"]["name"] == "advance"
    # tool result tied to the call id
    assert history[3] == {"role": "tool", "tool_call_id": "c1", "content": "t=5.0s"}
    # the nudge is a user message, exactly as recorded in the trajectory
    assert history[4] == {"role": "user", "content": "NUDGE TEXT"}


def test_error_results_are_marked_for_the_model(captured):
    calls, responses = captured
    responses.append(_response(tool_calls=[_tool_call("c1", "advance", "{}")]))
    responses.append(_response(content="ok", finish_reason="stop"))

    adapter = LiteLLMAdapter("some/model")
    turn1 = adapter.start(instructions="S", tools=MCP_TOOLS, user_message="U")
    adapter.next(
        [ToolResult(call=turn1.tool_calls[0], content="seconds must be positive", is_error=True)]
    )

    tool_msg = calls[1]["messages"][3]
    assert tool_msg["content"] == "ERROR: seconds must be positive"


def test_malformed_arguments_become_empty_args(captured):
    """Bad JSON from the model turns into {} — the tool schema rejects it and
    the refusal is recorded as the agent's error, not a harness crash. The
    original payload survives in raw so the trajectory keeps the evidence."""
    calls, responses = captured
    responses.append(
        _response(tool_calls=[_tool_call("c1", "advance", '{"seconds": NOT JSON')])
    )

    adapter = LiteLLMAdapter("some/model")
    turn = adapter.start(instructions="S", tools=MCP_TOOLS, user_message="U")
    assert turn.tool_calls[0].args == {}
    assert turn.raw["malformed_tool_arguments"] == {"c1": '{"seconds": NOT JSON'}


def test_non_object_arguments_are_treated_as_malformed(captured):
    """Valid JSON that is not an object (e.g. a bare list) cannot be tool args."""
    calls, responses = captured
    responses.append(_response(tool_calls=[_tool_call("c1", "advance", "[1, 2]")]))

    adapter = LiteLLMAdapter("some/model")
    turn = adapter.start(instructions="S", tools=MCP_TOOLS, user_message="U")
    assert turn.tool_calls[0].args == {}
    assert turn.raw["malformed_tool_arguments"] == {"c1": "[1, 2]"}


def test_two_tool_calls_in_one_turn_keep_order_and_history(captured):
    """A parallel-call turn: both calls surface in order, and the *next*
    request's history carries the assistant turn (content and both calls)
    followed by one tool message per result, tied by id."""
    calls, responses = captured
    responses.append(
        _response(
            content="doing both",
            tool_calls=[
                _tool_call("c1", "advance", '{"seconds": 5}'),
                _tool_call("c2", "read_ecam", "{}"),
            ],
        )
    )
    responses.append(_response(content="done", finish_reason="stop"))

    adapter = LiteLLMAdapter("some/model")
    turn = adapter.start(instructions="S", tools=MCP_TOOLS, user_message="U")
    assert [c.id for c in turn.tool_calls] == ["c1", "c2"]
    assert turn.text == "doing both"

    adapter.next(
        [
            ToolResult(call=turn.tool_calls[0], content="t=5.0s", is_error=False),
            ToolResult(call=turn.tool_calls[1], content="[]", is_error=False),
        ]
    )
    history = calls[1]["messages"]
    assistant = history[2]
    assert assistant["content"] == "doing both"
    assert [tc["id"] for tc in assistant["tool_calls"]] == ["c1", "c2"]
    assert history[3] == {"role": "tool", "tool_call_id": "c1", "content": "t=5.0s"}
    assert history[4] == {"role": "tool", "tool_call_id": "c2", "content": "[]"}


def test_turn_without_tool_calls_omits_the_key_in_history(captured):
    """OpenAI rejects an explicit tool_calls null on assistant messages, so a
    no-call turn must go back into history without the key at all."""
    calls, responses = captured
    responses.append(_response(content="thinking out loud", finish_reason="stop"))
    responses.append(_response(content="done", finish_reason="stop"))

    adapter = LiteLLMAdapter("some/model")
    adapter.start(instructions="S", tools=MCP_TOOLS, user_message="U")
    adapter.next([], nudge="NUDGE")

    assistant = calls[1]["messages"][2]
    assert assistant == {"role": "assistant", "content": "thinking out loud"}
    assert "tool_calls" not in assistant


def test_sampling_is_forwarded_to_completion(captured):
    calls, responses = captured
    responses.append(_response(content="ok", finish_reason="stop"))

    adapter = LiteLLMAdapter("some/model", sampling={"temperature": 0, "seed": 7})
    adapter.start(instructions="S", tools=MCP_TOOLS, user_message="U")

    assert calls[0]["temperature"] == 0
    assert calls[0]["seed"] == 7


def test_usage_lands_in_raw(captured):
    calls, responses = captured

    class FakeUsage:
        def model_dump(self):
            return {"prompt_tokens": 11, "completion_tokens": 3}

    response = _response(content="ok", finish_reason="stop")
    response.usage = FakeUsage()
    responses.append(response)

    adapter = LiteLLMAdapter("some/model")
    turn = adapter.start(instructions="S", tools=MCP_TOOLS, user_message="U")
    assert turn.raw["finish_reason"] == "stop"
    assert turn.raw["usage"] == {"prompt_tokens": 11, "completion_tokens": 3}


def test_info_records_the_translation_layer_version(captured):
    adapter = LiteLLMAdapter("some/model", sampling={"temperature": 0})
    assert adapter.info["provider"] == "litellm"
    assert adapter.info["sampling"] == {"temperature": 0}
    assert adapter.info["litellm_version"], "the pinned version must be recorded"


def test_cli_parser_shape():
    from a320_bench.cli import build_parser

    args = build_parser().parse_args(
        ["run", "--scenario", "s.yaml", "--model", "m", "--runs", "3", "--sampling", '{"temperature": 0}']
    )
    assert args.command == "run"
    assert args.runs == 3
    assert args.sampling == {"temperature": 0}


@pytest.mark.parametrize(
    "argv",
    [
        ["run", "--scenario", "s.yaml", "--model", "m", "--sampling", "{not json"],
        ["run", "--scenario", "s.yaml", "--model", "m", "--sampling", "[1, 2]"],
        ["run", "--scenario", "s.yaml", "--model", "m", "--runs", "0"],
    ],
)
def test_cli_rejects_bad_arguments(argv):
    from a320_bench.cli import build_parser

    with pytest.raises(SystemExit) as excinfo:
        build_parser().parse_args(argv)
    assert excinfo.value.code == 2
