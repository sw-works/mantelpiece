from mantelpiece_agents.model import ScriptedModel
from mantelpiece_agents.tools import InMemoryTools, Trip
from mantelpiece_agents.workflows import trip_curator, trivia_lines, write_trivia

GOOD_STORY = "Hanyu started at Kinkaku-ji. Maya arrived for Fushimi Inari. The last day was quiet."
SUMMARY = (
    "Kinkaku-ji is a Zen Buddhist temple in Kyoto, Japan. The top two floors are covered "
    "in gold leaf. A monk burned it down in 1950."
)


def with_facts(trip: Trip) -> Trip:
    trip.landmark_facts = {
        "kinkaku-ji": {"title": "Kinkaku-ji", "summary": SUMMARY, "has_trivia": False},
        "fushimi-inari": {"title": "Fushimi Inari", "summary": "Shrine.", "has_trivia": True},
    }
    return trip


def test_answers_become_card_lines_without_bullets() -> None:
    assert trivia_lines("1. Gold leaf.\n- Burned in 1950.\n\n* Zen.\nfour") == [
        "Gold leaf.",
        "Burned in 1950.",
        "Zen.",
    ]


def test_trivia_is_written_once_per_landmark_and_retried_with_the_reasons(
    tools: InMemoryTools, kyoto: Trip
) -> None:
    with_facts(kyoto)
    tools.trivia_refusals = [["number 1949 not in the source"]]
    model = ScriptedModel(["Burned in 1949.", "The top two floors are covered in gold leaf."])
    done = write_trivia(tools, model, "h", kyoto)
    assert done == {"kinkaku-ji": True}
    assert tools.trivia["kinkaku-ji"] == ["The top two floors are covered in gold leaf."]
    # The summary goes in; the retry carries the refusal.
    assert SUMMARY in model.prompts[0][1]
    assert "1949 not in the source" in model.prompts[1][1]


def test_twice_refused_trivia_is_left_to_the_summary(tools: InMemoryTools, kyoto: Trip) -> None:
    with_facts(kyoto)
    tools.trivia_refusals = [["unlisted name 'Mishima'"], ["unlisted name 'Mishima'"]]
    done = write_trivia(tools, ScriptedModel(["Mishima.", "Mishima again."]), "h", kyoto)
    assert done == {"kinkaku-ji": False}
    assert "kinkaku-ji" not in tools.trivia


def test_the_curator_writes_trivia_before_the_tv_hears_the_trip_is_ready(
    tools: InMemoryTools, kyoto: Trip
) -> None:
    with_facts(kyoto)
    trip_curator(tools, ScriptedModel([GOOD_STORY, "Covered in gold leaf."]), "h", "kyoto")
    trivia_at = next(i for i, e in enumerate(tools.log) if e.startswith("save_landmark_trivia"))
    assert tools.trivia["kinkaku-ji"] == ["Covered in gold leaf."]
    assert trivia_at < tools.log.index("push_event memory.ready")
