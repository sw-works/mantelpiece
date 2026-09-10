from memorylane_agents import workflows
from memorylane_agents.model import ScriptedModel
from memorylane_agents.runtime import invoke
from memorylane_agents.tools import InMemoryTools, NowShowing, PhotoAnswer


def showing(tools: InMemoryTools, media: str = "m1") -> None:
    tools.now_showing["tv-1"] = NowShowing(media, "kyoto", 2, True, "2026-09-08T10:00:00Z")


def test_answers_from_the_stored_description_without_the_model() -> None:
    tools = InMemoryTools()
    showing(tools)
    tools.descriptions["m1"] = PhotoAnswer(
        "The Great Buddha at Todai-ji.", "Todai-ji", "Maya", "2026-04-04"
    )
    model = ScriptedModel([])
    result = workflows.describe_photo(tools, model, "h", "what's that statue")
    assert result.mode == "describe" and result.answer == "The Great Buddha at Todai-ji."
    assert model.prompts == []
    assert tools.events[-1] == {
        "type": "answer.card",
        "question": "what's that statue",
        "answer": "The Great Buddha at Todai-ji.",
        "media": "m1",
    }


def test_model_fallback_is_checked_against_the_facts() -> None:
    tools = InMemoryTools()
    showing(tools)
    tools.descriptions["m1"] = PhotoAnswer("", "Todai-ji", "Maya", "2026-04-04")
    bad = ScriptedModel(["Maya visited Osaka Castle in 2024."])
    result = workflows.describe_photo(tools, bad, "h", "where is this")
    assert result.answer is None and result.problems
    assert tools.events == []
    good = ScriptedModel(["Maya took this at Todai-ji."])
    result = workflows.describe_photo(tools, good, "h", "where is this")
    assert result.answer == "Maya took this at Todai-ji."
    assert "taken by Maya on 2026-04-04 at Todai-ji" in good.prompts[0][1]


def test_nothing_on_screen_and_runtime_payload() -> None:
    tools = InMemoryTools()
    assert workflows.describe_photo(tools, ScriptedModel([]), "h", "what is this").mode == "nothing"
    showing(tools, "missing")
    assert workflows.describe_photo(tools, ScriptedModel([]), "h", "?").problems == [
        "photo not found"
    ]
    showing(tools)
    tools.descriptions["m1"] = PhotoAnswer("A quiet street.", None, "Hanyu", "2026-04-03")
    out = invoke(
        {"household": "h", "question": "what is this", "tv": "tv-1"}, tools, ScriptedModel([])
    )
    assert out["result"]["answer"] == "A quiet street."
