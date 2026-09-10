import pytest

from memorylane_agents import workflows
from memorylane_agents.model import ScriptedModel
from memorylane_agents.router import handle_event
from memorylane_agents.tools import CleanupCandidate, ConfirmationRequired, InMemoryTools


def test_quick_cleanup_proposes_and_applies_only_with_a_token(tools: InMemoryTools) -> None:
    tools.cleanup = [
        CleanupCandidate("m1", "screenshot", "kyoto"),
        CleanupCandidate("m2", "receipt", "kyoto"),
    ]
    tools.confirmations = [None]
    result = workflows.quick_cleanup(tools, "h", "kyoto")
    assert (result.candidates, result.reasons, result.applied, result.confirmed) == (
        2,
        ["receipt", "screenshot"],
        0,
        False,
    )
    assert tools.events[-1] == {
        "type": "cleanup.proposal",
        "trip": "kyoto",
        "count": 2,
        "reasons": ["receipt", "screenshot"],
    }
    assert len(tools.cleanup) == 2

    tools.confirmations = ["conf-7"]
    applied = handle_event(
        tools, ScriptedModel([]), "cleanup.candidates", {"household": "h", "trip": "kyoto"}
    )
    assert applied.confirmed and applied.applied == 2
    assert tools.cleanup == []
    assert "apply_cleanup 2 conf-7" in tools.log
    with pytest.raises(ConfirmationRequired):
        tools.apply_cleanup("h", ["m9"], "")
    assert workflows.quick_cleanup(tools, "h").candidates == 0


def test_nightly_tick_runs_patterns_then_purges(tools: InMemoryTools) -> None:
    tools.removed = ["m1", "m2"]
    out = handle_event(tools, ScriptedModel([]), "nightly.tick", {"household": "h"})
    assert out["purged"] == 2
    assert out["ambient_playlist"] in tools.playlists
    assert tools.log[-1] == "purge_expired"
