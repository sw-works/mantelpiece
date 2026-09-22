import pytest

from mantelpiece_agents import workflows
from mantelpiece_agents.model import ScriptedModel
from mantelpiece_agents.router import handle_event
from mantelpiece_agents.runtime import invoke
from mantelpiece_agents.tools import ConfirmationRequired, InMemoryTools, Trip

GOOD = "Hanyu started at Kinkaku-ji. Maya arrived for Fushimi Inari. The last day was quiet."
BAD = "Grandma flew in from Osaka in May."


def test_trip_curator_writes_story_playlist_and_events(tools: InMemoryTools) -> None:
    model = ScriptedModel([GOOD])
    result = workflows.trip_curator(tools, model, "h", "kyoto")
    assert result.narrative_ok
    assert tools.trips["kyoto"].narrative == {
        "text": GOOD,
        "model_id": "scripted",
        "check": "passed",
    }
    assert [d["story"] for d in tools.trips["kyoto"].days] == [
        "Hanyu started at Kinkaku-ji.",
        "Maya arrived for Fushimi Inari.",
        "The last day was quiet.",
    ]
    assert tools.playlists[result.playlist_id] == ["m2", "m4", "m6"]
    assert [e["type"] for e in tools.events] == ["memory.ready", "playlist.created"]


def test_trip_curator_retries_once_then_withholds_bad_story(tools: InMemoryTools) -> None:
    model = ScriptedModel([BAD, BAD])
    result = workflows.trip_curator(tools, model, "h", "kyoto")
    assert not result.narrative_ok
    assert tools.trips["kyoto"].narrative == {
        "text": "",
        "model_id": "scripted",
        "check": "withheld",
    }
    assert "withhold_narrative kyoto" in tools.log
    assert "not in the list" in model.prompts[1][1]
    assert len(model.prompts) == 2
    assert result.problems


def test_ask_archive_builds_playlist_from_intent(tools: InMemoryTools) -> None:
    model = ScriptedModel(["beach kids portugal"])
    result = workflows.ask_archive(tools, model, "h", "show beach days with the kids")
    assert result.query == "beach kids portugal"
    assert result.count == 3
    assert tools.playlists[result.playlist_id] == ["m4", "m9", "m4"]
    scoped = workflows.ask_archive(tools, ScriptedModel(["beach"]), "h", "beach", trip_id="kyoto")
    assert tools.playlists[scoped.playlist_id] == ["m4"]
    nothing = workflows.ask_archive(tools, ScriptedModel(["snow"]), "h", "snow")
    assert nothing.playlist_id is None and nothing.count == 0


def test_dedup_steward_never_applies_without_confirmation(tools: InMemoryTools) -> None:
    tools.confirmations = [None]
    result = workflows.dedup_steward(tools, "h", "kyoto")
    assert (result.groups, result.removable, result.applied, result.confirmed) == (2, 3, 0, False)
    assert tools.events[-1]["type"] == "dedup.proposal"
    assert len(tools.duplicates) == 2

    tools.confirmations = ["conf-1"]
    applied = workflows.dedup_steward(tools, "h", "kyoto")
    assert applied.confirmed and applied.applied == 3
    assert tools.duplicates == []
    assert "apply_dedup 2 conf-1" in tools.log

    with pytest.raises(ConfirmationRequired):
        tools.apply_dedup("h", ["g1"], "")


def test_household_patterns_learns_from_feedback(tools: InMemoryTools) -> None:
    pid = workflows.household_patterns(
        tools,
        "h",
        feedback=[{"action": "pin", "tags": ["beach"]}, {"action": "hide", "tags": ["temple"]}],
    )
    assert tools.memory["liked_tags"] == {"beach": 1, "temple": -1}
    assert tools.playlists[pid] == ["m4", "m9"]
    assert "favourite tag beach" in tools.memory["ambient_rationale"]
    no_feedback = InMemoryTools(trips=tools.trips)
    pid2 = workflows.household_patterns(no_feedback, "h")
    assert no_feedback.playlists[pid2] == ["m2", "m4", "m6"]


def test_share_requires_confirmation_then_links(tools: InMemoryTools) -> None:
    tools.confirmations = [None]
    declined = workflows.share(tools, ScriptedModel(["beach"]), "h", "send beach photos to grandma")
    assert declined.playlist_id == "pl-1" and declined.link is None
    tools.confirmations = ["conf-9"]
    out = workflows.share(tools, ScriptedModel(["beach"]), "h", "send beach photos to grandma")
    assert out.link == "https://share.test/send-beach-photos-to-grandma"
    assert out.join_code is None
    assert "share_album 'send beach photos to grandma' 2 conf-9" in tools.log


def test_share_offers_a_join_code_when_the_recipient_has_a_tv(
    tools: InMemoryTools, kyoto: Trip
) -> None:
    tools.memory["contacts"] = {
        "Grandma": {"household": "amzn-ruth"},
        "Uncle Bo": {"email": "bo@example.com"},
    }
    out = workflows.share(tools, ScriptedModel(["temple"]), "h", "send the Kyoto trip to grandma")
    assert out.join_code == "CODE-KYOTO" and out.recipient == "Grandma" and out.link is None
    assert tools.events[-1]["type"] == "share.join_code"
    assert "request_confirmation share_album" not in tools.log

    tools.confirmations = ["conf-2"]
    out = workflows.share(tools, ScriptedModel(["temple"]), "h", "send Kyoto temples to Uncle Bo")
    assert out.link and out.recipient == "Uncle Bo"
    assert "share_album 'send Kyoto temples to Uncle Bo' 2 conf-2 to bo@example.com" in tools.log


def test_router_and_runtime_payloads(tools: InMemoryTools, kyoto: Trip) -> None:
    model = ScriptedModel([GOOD, GOOD, "temple"])
    out = invoke(
        {"detail-type": "trip.detected", "detail": {"household": "h", "trip": "kyoto"}},
        tools,
        model,
    )
    assert out["handled"] and out["result"]["narrative_ok"]
    merged = handle_event(
        tools,
        model,
        "contribution.received",
        {
            "household": "h",
            "trip": "kyoto",
            "contributor": "ken",
            "display_name": "Ken",
            "count": 7,
        },
    )
    assert merged.trip_id == "kyoto"
    assert tools.events[-3]["display_name"] == "Ken"
    asked = invoke({"household": "h", "prompt": "temple shots"}, tools, model)
    assert asked["result"]["count"] == 2
    assert (
        invoke({"detail-type": "unknown", "detail": {"household": "h"}}, tools, model)["handled"]
        is False
    )
    assert invoke({}, tools, model)["error"]
    assert (
        workflows.summarize_trip(kyoto) == "Kyoto, April 2026: 3 days, 6 photos from Hanyu, Maya."
    )
