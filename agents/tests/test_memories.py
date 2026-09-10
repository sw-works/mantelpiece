from memorylane_agents import workflows
from memorylane_agents.model import ScriptedModel
from memorylane_agents.router import handle_event
from memorylane_agents.tools import InMemoryTools, Memories


def test_memories_become_playlists_and_an_event(tools: InMemoryTools) -> None:
    tools.memories["today"] = Memories(
        date="2026-09-08",
        on_this_day=["m1", "m2"],
        then_and_now=[{"then": "m1", "now": "m4", "years_apart": 2}],
        best_of=["m4", "m6"],
    )
    result = workflows.memories_nightly(tools, "h")
    assert (result.on_this_day, result.then_and_now) == (2, 1)
    assert len(result.playlists) == 2
    assert tools.playlists[result.playlists[0]] == ["m1", "m2"]
    assert tools.events[-1] == {
        "type": "memories.ready",
        "date": "2026-09-08",
        "on_this_day": 2,
        "then_and_now": 1,
    }


def test_quiet_day_makes_no_noise(tools: InMemoryTools) -> None:
    before = len(tools.events)
    result = workflows.memories_nightly(tools, "h", "2026-01-01")
    assert result.playlists == [] and len(tools.events) == before


def test_nightly_tick_includes_memories(tools: InMemoryTools) -> None:
    tools.memories["today"] = Memories(date="2026-09-08", on_this_day=["m2"])
    out = handle_event(tools, ScriptedModel([]), "nightly.tick", {"household": "h"})
    assert out["on_this_day"] == 1 and len(out["memories"]) == 1
    assert tools.log[-1] == "purge_expired"
