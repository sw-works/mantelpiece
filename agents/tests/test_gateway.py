"""The Gateway client, driven through a scripted transport: no Gateway, no network."""

import json

import pytest

from mantelpiece_agents.gateway import GatewayTools, ToolError
from mantelpiece_agents.model import ScriptedModel
from mantelpiece_agents.workflows import trip_curator

TRIP = {
    "id": "kyoto",
    "household": "tv-1",
    "name": "Kyoto, April 2026",
    "start": "2026-04-03T00:00:00Z",
    "end": "2026-04-04T00:00:00Z",
    "days": [{"index": 1, "date": "2026-04-03", "media": ["m1", "m2"], "title": "Day 1: Kyoto"}],
    "highlights": ["m1"],
    "contributors": ["c1"],
    "narrative": None,
    "landmarks": ["Kyoto"],
    "contributor_names": {"c1": "Maya"},
    # The API says more than the agent needs.
    "route": {"points": []},
    "hero": "m1",
    "media_credits": {"m1": "c1"},
}


class Wire:
    def __init__(self, answers):
        self.answers = answers
        self.seen = []
        self.tokens = 0

    def __call__(self, url, headers, body):
        if url.endswith("/oauth2/token"):
            self.tokens += 1
            assert headers["authorization"].startswith("Basic ")
            assert b"grant_type=client_credentials" in body and b"mantelpiece%2Ftools" in body
            return 200, json.dumps({"access_token": f"tok{self.tokens}", "expires_in": 3600})
        call = json.loads(body)
        assert call["method"] == "tools/call"
        self.seen.append(
            (headers["authorization"], call["params"]["name"], call["params"]["arguments"])
        )
        return self.answers[call["params"]["name"].split("___")[1]](call)


def text_result(value, sse=False):
    def answer(call):
        message = {
            "jsonrpc": "2.0",
            "id": call["id"],
            "result": {"content": [{"type": "text", "text": json.dumps(value)}]},
        }
        return 200, (
            f"event: message\ndata: {json.dumps(message)}\n\n" if sse else json.dumps(message)
        )

    return answer


def tools(wire, clock=lambda: 1000.0):
    return GatewayTools(
        "https://gw/mcp", "https://auth/oauth2/token", "id", "secret", transport=wire, clock=clock
    )


def test_the_curator_writes_a_story_through_the_gateway():
    wire = Wire(
        {
            "get_trip": text_result(TRIP, sse=True),
            "save_narrative": text_result({"check": "passed"}),
            "get_household": text_result({"household_name": "Den", "narration_enabled": False}),
            "create_playlist": text_result({"id": "pl-9", "name": "x"}),
            "push_event": text_result({"seq": 4}),
        }
    )
    model = ScriptedModel(["Day one in Kyoto, Maya walked the old streets."])
    result = trip_curator(tools(wire), model, "tv-1", "kyoto")
    assert result.narrative_ok and result.playlist_id == "pl-9"
    names = [name for _, name, _ in wire.seen]
    assert names == [
        "api___get_trip",
        "api___save_narrative",
        "api___get_household",
        "api___create_playlist",
        "api___push_event",
        "api___push_event",
    ]
    # One sign-in serves every call, and absent arguments are left out.
    assert wire.tokens == 1 and {auth for auth, _, _ in wire.seen} == {"Bearer tok1"}
    # Structured arguments travel as JSON text, which is how the Gateway's schema types them.
    assert json.loads(wire.seen[4][2]["event"]) == {"type": "memory.ready", "trip": "kyoto"}
    saved = wire.seen[1][2]
    assert saved["stories"] == ["Day one in Kyoto, Maya walked the old streets."]
    assert "withhold" not in saved


def test_a_token_is_renewed_when_it_runs_out_and_refusals_raise():
    now = [1000.0]
    wire = Wire(
        {
            "list_trips": text_result([TRIP]),
            "read_memory": text_result({"error": "501: read_memory is not available yet"}),
        }
    )
    gateway = tools(wire, clock=lambda: now[0])
    assert [t.id for t in gateway.list_trips("tv-1")] == ["kyoto"]
    now[0] += 3600
    gateway.list_trips("tv-1")
    assert wire.tokens == 2
    with pytest.raises(ToolError, match="not available yet"):
        gateway.read_memory("tv-1")


def test_gateway_errors_and_denials_raise():
    def denied(call):
        return 200, json.dumps(
            {
                "jsonrpc": "2.0",
                "id": call["id"],
                "error": {"code": -32002, "message": "Tool Execution Denied"},
            }
        )

    def tool_error(call):
        return 200, json.dumps(
            {
                "jsonrpc": "2.0",
                "id": call["id"],
                "result": {"isError": True, "content": [{"type": "text", "text": "boom"}]},
            }
        )

    gateway = tools(
        Wire(
            {
                "apply_dedup": denied,
                "purge_expired": tool_error,
                "list_trips": lambda call: (403, "no"),
            }
        )
    )
    with pytest.raises(ToolError, match="Denied"):
        gateway.apply_dedup("tv-1", ["g1"], "")
    with pytest.raises(ToolError, match="boom"):
        gateway.purge_expired("tv-1")
    with pytest.raises(ToolError, match="403"):
        gateway.list_trips("tv-1")


def test_refused_trivia_comes_back_as_reasons_not_an_exception():
    wire = Wire(
        {
            "save_landmark_trivia": text_result(
                {"error": "trivia refused: number 1234 not in the source"}
            )
        }
    )
    problems = tools(wire).save_landmark_trivia("tv-1", "kinkaku-ji", ["Built in 1234."])
    assert problems and "1234" in problems[0]
    assert wire.seen[0][2] == {
        "household": "tv-1",
        "landmark": "kinkaku-ji",
        "lines": ["Built in 1234."],
    }
    kept = Wire({"save_landmark_trivia": text_result({"key": "kinkaku-ji", "has_trivia": True})})
    assert tools(kept).save_landmark_trivia("tv-1", "kinkaku-ji", ["Gold leaf."]) == []
