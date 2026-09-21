from memorylane_agents.narrative import (
    Facts,
    check_narrative,
    facts_for,
    prompt_for,
    split_days,
)
from memorylane_agents.tools import Trip


def test_prompt_lists_only_metadata(kyoto: Trip) -> None:
    prompt = prompt_for(kyoto)
    assert "Kyoto, April 2026" in prompt
    assert "Hanyu, Maya" in prompt
    assert "Day 2: Fushimi Inari (2026-04-04): 2 photos, by Hanyu, Maya" in prompt
    assert "  - Kinkaku-ji" in prompt


def test_check_accepts_text_within_facts(kyoto: Trip) -> None:
    facts = facts_for(kyoto)
    text = (
        "On the first day Hanyu walked up to Kinkaku-ji in the April light. "
        "The next morning Maya joined at Fushimi Inari and the gates went on forever. "
        "Day three was a slow one, mostly Maya and coffee."
    )
    assert check_narrative(text, facts) == []


def test_check_flags_invented_places_dates_and_people(kyoto: Trip) -> None:
    facts = facts_for(kyoto)
    problems = check_narrative(
        "In May 2025 Grandma took everyone to Osaka Castle before lunch near Kinkaku-ji.", facts
    )
    assert any("2025" in p for p in problems)
    assert any("month may" in p for p in problems)
    assert any("Grandma" in p for p in problems)
    assert any("Osaka Castle" in p for p in problems)
    assert not any("Kinkaku-ji" in p for p in problems)


def test_split_days_prefers_paragraphs_then_deals_sentences() -> None:
    assert split_days("One.\n\nTwo.\n\nThree.", 3) == ["One.", "Two.", "Three."]
    assert split_days("A. B. C. D. E.", 2) == ["A. B. C.", "D. E."]
    assert split_days("Only one.", 3) == ["Only one.", "", ""]
    assert split_days("", 2) == ["", ""]
    assert split_days("x", 0) == []


def test_sentence_openers_and_paragraph_starts_are_not_names():
    facts = Facts(
        places={"Skógafoss", "Iceland"}, dates={"2015-06-27", "2015-07-01"}, people={"Hanyu"}
    )
    text = (
        "On June 27th the family reached Iceland. Finally they saw Skógafoss.\n\n"
        'On July 1st Hanyu walked behind the falls. "Next time," they said, "we stay longer."'
    )
    assert check_narrative(text, facts) == []
    # Forms of a listed name are that name; near-misses are not.
    assert check_narrative("They loved the Icelandic light.", facts) == []
    assert check_narrative("They loved the Irish light.", facts) == ["unlisted name 'Irish'"]
    # A real invention after a sentence opener is still caught.
    assert check_narrative("On June 27th they reached Iceland.\n\nThen Paris called.", facts) == [
        "unlisted name 'Paris'"
    ]
