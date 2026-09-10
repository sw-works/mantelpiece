from memorylane_agents import workflows
from memorylane_agents.model import ScriptedModel
from memorylane_agents.tools import InMemoryTools

GOOD = "Hanyu started at Kinkaku-ji. Maya arrived for Fushimi Inari. The last day was quiet."


def test_narration_only_when_the_household_switched_it_on(tools: InMemoryTools) -> None:
    workflows.trip_curator(tools, ScriptedModel([GOOD]), "h", "kyoto")
    assert tools.narrations == []
    tools.settings["narration_enabled"] = True
    workflows.trip_curator(tools, ScriptedModel([GOOD]), "h", "kyoto")
    assert tools.narrations == [("kyoto", 1), ("kyoto", 2), ("kyoto", 3)]
    assert "synthesize_narration kyoto 2" in tools.log
