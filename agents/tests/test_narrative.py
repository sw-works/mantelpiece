from memorylane_agents.narrative import check_narrative, facts_for, prompt_for
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
