from memorylane_agents import tracing
from memorylane_agents.gateway import GatewayTools
from memorylane_agents.model import ScriptedModel
from memorylane_agents.runtime import invoke
from memorylane_agents.tools import InMemoryTools, Trip

GOOD = "Hanyu started at Kinkaku-ji. Maya arrived for Fushimi Inari. The last day was quiet."


class FakeMcp:
    def __init__(self) -> None:
        self.calls: list[str] = []

    def call_tool_sync(self, tool_use_id: str, name: str, arguments: dict) -> dict:
        self.calls.append(name)
        return {
            "content": [
                {
                    "json": [
                        {
                            "id": "kyoto",
                            "name": "Kyoto",
                            "start": "",
                            "end": "",
                            "days": [],
                            "highlights": [],
                            "contributors": [],
                        }
                    ]
                }
            ]
        }


def test_every_run_and_tool_call_is_a_span_tagged_with_household_and_workflow(
    tools: InMemoryTools, kyoto: Trip
) -> None:
    rec = tracing.RecordingTracer()
    tracing.use_tracer(rec)
    try:
        invoke(
            {"detail-type": "trip.detected", "detail": {"household": "h", "trip": "kyoto"}},
            tools,
            ScriptedModel([GOOD]),
        )
        invoke({"household": "h", "prompt": "beach days"}, tools, ScriptedModel(["beach"]))
        invoke(
            {"household": "h", "question": "what is that", "tv": "tv-1"}, tools, ScriptedModel([])
        )
        gateway = GatewayTools(url="https://gateway.test/mcp", token_provider="cognito")
        gateway._client = FakeMcp()  # noqa: SLF001 - the MCP client is replaced for the test
        gateway.list_trips("h")
    finally:
        tracing.use_tracer(None)
    names = [s.name for s in rec.spans]
    assert names == [
        "agent.trip_curator",
        "agent.ask_archive",
        "agent.describe_photo",
        "tool.list_trips",
    ]
    assert all(s.attributes[tracing.ATTR_HOUSEHOLD] == "h" for s in rec.spans)
    assert rec.spans[0].attributes[tracing.ATTR_WORKFLOW] == "trip_curator"
    assert rec.spans[0].attributes[tracing.ATTR_TRIGGER] == "trip.detected"
    assert rec.spans[3].attributes[tracing.ATTR_TOOL] == "list_trips"
    assert all(s.error is None for s in rec.spans)


def test_a_failing_run_still_closes_its_span_with_the_error() -> None:
    rec = tracing.RecordingTracer()
    try:
        with rec.span("agent.x", {}):
            raise RuntimeError("boom")
    except RuntimeError:
        pass
    assert rec.spans[0].error == "RuntimeError: boom"
    assert tracing.workflow_for("nightly.tick") == "nightly"
