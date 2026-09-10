"""Day-by-day trip narrative from structured data only (F-301, F-302).

The prompt lists exactly the places, dates and contributors the metadata contains, and
:func:`check_narrative` rejects output that mentions anything else, so the story never
invents a stop the family did not make.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from .tools import Trip

SYSTEM = (
    "You write short, warm, day-by-day accounts of family trips for a living-room TV. "
    "Use only the places, dates and people listed. Never add a place, date or person that "
    "is not listed. Two or three sentences per day. Plain prose, no headings, no lists."
)

MONTHS = [
    "january", "february", "march", "april", "may", "june",
    "july", "august", "september", "october", "november", "december",
]  # fmt: skip
WEEKDAYS = {"monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"}


@dataclass
class Facts:
    places: set[str]
    dates: set[str]
    people: set[str]


def facts_for(trip: Trip) -> Facts:
    places = set(trip.landmarks)
    dates = {d["date"] for d in trip.days}
    people = set(trip.contributor_names.values())
    return Facts(places, dates, people)


def prompt_for(trip: Trip) -> str:
    lines = [f"Trip: {trip.name}", f"Dates: {trip.start[:10]} to {trip.end[:10]}"]
    if trip.contributor_names:
        lines.append(
            "People who took photos: " + ", ".join(sorted(trip.contributor_names.values()))
        )
    if trip.landmarks:
        lines.append("Places identified in the photos: " + ", ".join(sorted(trip.landmarks)))
    lines.append("")
    for d in trip.days:
        title = d.get("title") or f"Day {d['index']}"
        who = ", ".join(
            sorted({trip.contributor_names.get(c, c) for c in d.get("contributors", [])})
        )
        lines.append(
            f"{title} ({d['date']}): {len(d['media'])} photos" + (f", by {who}" if who else "")
        )
        for place in d.get("landmarks", []):
            lines.append(f"  - {place}")
    return "\n".join(lines)


def split_days(text: str, days: int) -> list[str]:
    """One paragraph per day for narration sync (F-709). Paragraphs separated by blank
    lines win; otherwise sentences are dealt out evenly. Always returns ``days`` items."""
    if days <= 0:
        return []
    paragraphs = [p.strip() for p in re.split(r"\n\s*\n", text.strip()) if p.strip()]
    if len(paragraphs) == days:
        return paragraphs
    sentences = [s.strip() for s in re.split(r"(?<=[.!?])\s+", text.strip()) if s.strip()]
    if not sentences:
        return [""] * days
    out: list[str] = []
    per, extra = divmod(len(sentences), days)
    start = 0
    for i in range(days):
        take = per + (1 if i < extra else 0)
        out.append(" ".join(sentences[start : start + take]))
        start += take
    return out


_YEAR = re.compile(r"\b(19|20)\d{2}\b")
_CAPITALISED = re.compile(r"\b[A-Z][a-zA-Z\-']+(?:\s+[A-Z][a-zA-Z\-']+)*\b")


def check_narrative(text: str, facts: Facts) -> list[str]:
    """Return a list of problems; empty means the text stays within the facts."""
    problems: list[str] = []
    years = {d[:4] for d in facts.dates}
    for m in _YEAR.finditer(text):
        if m.group(0) not in years:
            problems.append(f"year {m.group(0)} not in trip dates")
    allowed_months = {MONTHS[int(d[5:7]) - 1] for d in facts.dates}
    for month in MONTHS:
        if re.search(rf"\b{month}\b", text, re.IGNORECASE) and month not in allowed_months:
            problems.append(f"month {month} not in trip dates")
    known = {p.lower() for p in facts.places | facts.people} | allowed_months | WEEKDAYS
    known_words = {w.lower() for p in facts.places | facts.people for w in p.split()}
    known_words |= allowed_months | WEEKDAYS | {"day", "days", "the", "we", "our"}
    for m in _CAPITALISED.finditer(text):
        phrase = m.group(0)
        if m.start() == 0 or text[m.start() - 2 : m.start()] in {". ", "! ", "? "}:
            continue  # sentence starts are ordinary words
        if phrase.lower() in known or all(w.lower() in known_words for w in phrase.split()):
            continue
        problems.append(f"unlisted name {phrase!r}")
    return problems
