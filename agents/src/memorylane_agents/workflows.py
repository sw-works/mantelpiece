"""The six agentic workflows (architecture section 6).

Every irreversible step goes through ``request_confirmation``: the TV opens Review
Changes, the user confirms on the remote, and only that token unlocks the tool.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Any

from .model import Model
from .narrative import SYSTEM, Facts, check_narrative, facts_for, prompt_for, split_days
from .tools import Tools, Trip

HIGHLIGHTS_IN_PLAYLIST = 12
NARRATIVE_ATTEMPTS = 2


@dataclass
class CuratorResult:
    trip_id: str
    playlist_id: str
    narrative_ok: bool
    problems: list[str]


def trip_curator(tools: Tools, model: Model, household: str, trip_id: str) -> CuratorResult:
    """W1: on ``trip.detected``, write the story, build the playlist, tell the TV."""
    trip = tools.get_trip(household, trip_id)
    facts = facts_for(trip)
    problems: list[str] = []
    text = ""
    for attempt in range(NARRATIVE_ATTEMPTS):
        prompt = prompt_for(trip)
        if attempt:
            prompt += "\n\nYour previous draft mentioned things not in the list: " + "; ".join(
                problems
            )
        text = model.complete(SYSTEM, prompt)
        problems = check_narrative(text, facts)
        if not problems:
            break
    ok = not problems
    if ok:
        stories = split_days(text, len(trip.days))
        tools.save_narrative(household, trip_id, text, model.model_id, stories=stories)
        # F-709: read the story aloud only where the household switched narration on.
        if tools.get_household(household).get("narration_enabled"):
            for day, story in zip(trip.days, stories, strict=False):
                if story.strip():
                    tools.synthesize_narration(household, trip_id, int(day["index"]))
    else:
        # F-301: nothing invented reaches the screen; the TV keeps "being written".
        tools.withhold_narrative(household, trip_id, model.model_id)
    playlist = tools.create_playlist(
        household,
        f"{trip.name} highlights",
        trip.highlights[:HIGHLIGHTS_IN_PLAYLIST],
        source=f"trip_curator:{trip_id}",
    )
    # F-210: trivia for the places the trip went, before the TV hears it is ready.
    write_trivia(tools, model, household, trip)
    tools.push_event(household, {"type": "memory.ready", "trip": trip_id})
    tools.push_event(
        household,
        {"type": "playlist.created", "playlist": playlist, "name": f"{trip.name} highlights"},
    )
    return CuratorResult(trip_id, playlist, ok, problems)


TRIVIA_SYSTEM = (
    "You write trivia cards shown on a TV while a family watches their holiday photos. "
    "Use only facts stated in the summary you are given. Answer with one to three lines, "
    "one fact per line, each under 140 characters, no numbering, no preamble. Keep every "
    "number, date and name exactly as the summary has it."
)
TRIVIA_ATTEMPTS = 2
TRIVIA_PER_TRIP = 6


def trivia_lines(raw: str) -> list[str]:
    """Model answer to card lines: bullets and numbering dropped, at most three."""
    lines = []
    for line in raw.splitlines():
        line = re.sub(r"^\s*(?:[-*\u2022]|\d+[.)])\s*", "", line).strip()
        if line:
            lines.append(line)
    return lines[:3]


def write_trivia(tools: Tools, model: Model, household: str, trip: Trip) -> dict[str, bool]:
    """Trivia for each landmark that has facts but no trivia yet. The API checks every
    line against the summary; a refused draft is retried once with the reasons, then left
    to the summary's own opening lines. Returns which landmarks got trivia."""
    done: dict[str, bool] = {}
    pending = [
        (key, facts)
        for key, facts in trip.landmark_facts.items()
        if not facts.get("has_trivia") and facts.get("summary")
    ]
    for key, facts in pending[:TRIVIA_PER_TRIP]:
        prompt = f"Landmark: {facts.get('title') or key}\nSummary: {facts['summary']}"
        problems: list[str] = []
        done[key] = False
        for attempt in range(TRIVIA_ATTEMPTS):
            ask = prompt
            if attempt:
                ask += "\n\nYour previous lines were refused: " + "; ".join(problems)
            lines = trivia_lines(model.complete(TRIVIA_SYSTEM, ask))
            if not lines:
                break
            problems = tools.save_landmark_trivia(household, key, lines)
            if not problems:
                done[key] = True
                break
    return done


INTENT_SYSTEM = (
    "Turn the user's request about their photo archive into a search query. Answer with "
    "one line of lowercase keywords: scenes, places, people, dates. No explanation."
)


@dataclass
class AskResult:
    query: str
    playlist_id: str | None
    count: int


def ask_archive(
    tools: Tools, model: Model, household: str, prompt: str, trip_id: str | None = None
) -> AskResult:
    """W2: prompt from TV or Alexa+ becomes a playlist the TV plays."""
    query = model.complete(INTENT_SYSTEM, prompt).strip().lower()
    media = tools.search_media(household, query, trip_id)
    if not media:
        return AskResult(query, None, 0)
    playlist = tools.create_playlist(household, prompt[:60], media, source=f"ask:{query}")
    tools.push_event(
        household, {"type": "playlist.created", "playlist": playlist, "name": prompt[:60]}
    )
    return AskResult(query, playlist, len(media))


DESCRIBE_SYSTEM = (
    "You answer one question about a family photo for a living-room TV. Use only the facts "
    "given. One or two sentences. If the facts do not answer the question, say what the "
    "photo shows instead. Never add a place, date or person that is not listed."
)


@dataclass
class DescribeResult:
    mode: str  # describe | nothing
    answer: str | None
    media: str | None
    problems: list[str]


def describe_photo(
    tools: Tools, model: Model, household: str, question: str, tv: str | None = None
) -> DescribeResult:
    """Voice question about the photo on screen (F-405, research R3 and R8).

    The stored description answers first, within the Alexa+ budget. Only when there is
    none does the model speak, from the photo's facts, and its answer goes through the same
    check as a story (constitution IV)."""
    now = tools.get_now_showing(household, tv)
    if now is None:
        return DescribeResult("nothing", None, None, [])
    try:
        stored = tools.describe_photo(household, now.media, question)
    except KeyError:
        return DescribeResult("nothing", None, now.media, ["photo not found"])
    answer: str | None = stored.answer or None
    problems: list[str] = []
    if not answer:
        facts = Facts(
            places={stored.landmark} if stored.landmark else set(),
            dates={stored.when} if stored.when else set(),
            people={stored.taken_by} if stored.taken_by else set(),
        )
        prompt = (
            f"Facts: taken by {stored.taken_by} on {stored.when}"
            + (f" at {stored.landmark}" if stored.landmark else "")
            + f".\nQuestion: {question}"
        )
        candidate = model.complete(DESCRIBE_SYSTEM, prompt)
        problems = check_narrative(candidate, facts)
        answer = None if problems else candidate
    if answer:
        tools.push_event(
            household,
            {"type": "answer.card", "question": question, "answer": answer, "media": now.media},
        )
    return DescribeResult("describe", answer, now.media, problems)


def household_patterns(
    tools: Tools, household: str, feedback: list[dict[str, Any]] | None = None
) -> str:
    """W3: nightly. Fold pin/hide feedback into memory, pick the ambient set, record why."""
    memory = tools.read_memory(household)
    liked: dict[str, int] = dict(memory.get("liked_tags", {}))
    for f in feedback or []:
        for tag in f.get("tags", []):
            liked[tag] = liked.get(tag, 0) + (1 if f.get("action") == "pin" else -1)
    tools.write_memory(household, "liked_tags", liked)
    favourite = max(liked, key=liked.get) if liked else None
    trips = tools.list_trips(household)
    media: list[str] = []
    for trip in trips:
        media.extend(trip.highlights[:4])
    if favourite:
        media = tools.search_media(household, favourite) or media
    rationale = f"ambient from {'favourite tag ' + favourite if favourite else 'recent highlights'}"
    playlist = tools.create_playlist(household, "Ambient", media, source=rationale)
    tools.write_memory(household, "ambient_rationale", rationale)
    return playlist


@dataclass
class DedupResult:
    groups: int
    removable: int
    applied: int
    confirmed: bool


def dedup_steward(tools: Tools, household: str, trip_id: str | None = None) -> DedupResult:
    """W4: propose keepers, open Review Changes, apply only with the user's token."""
    groups = tools.list_duplicates(household, trip_id)
    removable = sum(len(g.members) - 1 for g in groups)
    if removable == 0:
        return DedupResult(0, 0, 0, False)
    tools.push_event(
        household,
        {"type": "dedup.proposal", "trip": trip_id, "groups": len(groups), "removable": removable},
    )
    action = {"action": "apply_dedup", "group_ids": [g.id for g in groups]}
    token = tools.request_confirmation(household, action)
    if not token:
        return DedupResult(len(groups), removable, 0, False)
    applied = tools.apply_dedup(household, action["group_ids"], token)
    return DedupResult(len(groups), removable, applied, True)


@dataclass
class CleanupResult:
    candidates: int
    reasons: list[str]
    applied: int
    confirmed: bool


def quick_cleanup(tools: Tools, household: str, trip_id: str | None = None) -> CleanupResult:
    """F-705: screenshots, documents, receipts and blurry frames become one proposal; the
    remote decides, and only its token removes anything."""
    candidates = tools.list_cleanup_candidates(household, trip_id)
    if not candidates:
        return CleanupResult(0, [], 0, False)
    reasons = sorted({c.reason for c in candidates})
    tools.push_event(
        household,
        {"type": "cleanup.proposal", "trip": trip_id, "count": len(candidates), "reasons": reasons},
    )
    action = {"action": "apply_cleanup", "media_ids": [c.media for c in candidates]}
    token = tools.request_confirmation(household, action)
    if not token:
        return CleanupResult(len(candidates), reasons, 0, False)
    applied = tools.apply_cleanup(household, action["media_ids"], token)
    return CleanupResult(len(candidates), reasons, applied, True)


@dataclass
class MemoriesResult:
    date: str
    on_this_day: int
    then_and_now: int
    playlists: list[str]


def memories_nightly(tools: Tools, household: str, date: str | None = None) -> MemoriesResult:
    """F-701 to F-703: turn today's resurfacing sets into playlists and tell the TV."""
    memories = tools.memories_for(household, date)
    playlists: list[str] = []
    if memories.on_this_day:
        playlists.append(
            tools.create_playlist(
                household,
                f"On this day, {memories.date}",
                memories.on_this_day,
                source=f"on_this_day:{memories.date}",
            )
        )
    if memories.best_of:
        playlists.append(
            tools.create_playlist(
                household, "Best of the month", memories.best_of, source=f"best_of:{memories.date}"
            )
        )
    if memories.on_this_day or memories.then_and_now:
        tools.push_event(
            household,
            {
                "type": "memories.ready",
                "date": memories.date,
                "on_this_day": len(memories.on_this_day),
                "then_and_now": len(memories.then_and_now),
            },
        )
    return MemoriesResult(
        memories.date, len(memories.on_this_day), len(memories.then_and_now), playlists
    )


def nightly(
    tools: Tools, household: str, feedback: list[dict[str, Any]] | None = None
) -> dict[str, Any]:
    """The Concierge's nightly tick: household patterns, today's memories, then purge
    what expired (contracts/events.md: the only consumer of nightly.tick)."""
    playlist = household_patterns(tools, household, feedback)
    memories = memories_nightly(tools, household)
    purged = tools.purge_expired(household)
    return {
        "ambient_playlist": playlist,
        "memories": memories.playlists,
        "on_this_day": memories.on_this_day,
        "purged": purged,
    }


@dataclass
class ShareResult:
    playlist_id: str | None
    """A join code for the recipient's own MemoryLane TV, offered before any link (W5)."""
    join_code: str | None = None
    link: str | None = None
    recipient: str | None = None


def known_contact(tools: Tools, household: str, request: str) -> tuple[str, dict[str, Any]] | None:
    """The first remembered contact named in the request. Memory key ``contacts`` maps a
    name to ``{"household": id?, "email": address?}``; the household id means they have a
    TV of their own."""
    contacts = tools.read_memory(household).get("contacts") or {}
    words = request.lower()
    for name, info in contacts.items():
        if name.lower() in words and isinstance(info, dict):
            return name, info
    return None


def trip_named_in(tools: Tools, household: str, request: str) -> str | None:
    """The trip whose name shares a word of four letters or more with the request, so
    "Kyoto, April 2026" matches "send the Kyoto trip to grandma"."""
    words = set(re.findall(r"[a-z]{4,}", request.lower()))
    for trip in tools.list_trips(household):
        if words & set(re.findall(r"[a-z]{4,}", trip.name.lower())):
            return trip.id
    return None


def share(tools: Tools, model: Model, household: str, request: str) -> ShareResult:
    """W5: "send Kyoto highlights to grandma". A recipient with a MemoryLane TV gets the
    trip's join code (their TV then shows the whole trip, F-162); anyone else gets an
    expiring link after confirmation, emailed when the address is remembered."""
    contact = known_contact(tools, household, request)
    trip_id = trip_named_in(tools, household, request)
    if contact and contact[1].get("household") and trip_id:
        code = tools.get_trip_code(household, trip_id)
        tools.push_event(
            household,
            {"type": "share.join_code", "trip": trip_id, "recipient": contact[0], "code": code},
        )
        return ShareResult(playlist_id=None, join_code=code, recipient=contact[0])
    result = ask_archive(tools, model, household, request, trip_id)
    if not result.playlist_id:
        return ShareResult(playlist_id=None)
    media = tools.search_media(household, result.query, trip_id)
    name = request[:60]
    token = tools.request_confirmation(
        household, {"action": "share_album", "album_id": result.playlist_id}
    )
    if not token:
        return ShareResult(playlist_id=result.playlist_id)
    email = contact[1].get("email") if contact else None
    link = tools.share_album(
        household, media, name, token, playlist=result.playlist_id, email=email
    )
    return ShareResult(
        playlist_id=result.playlist_id,
        link=link,
        recipient=contact[0] if contact else None,
    )


def trip_merge(
    tools: Tools,
    model: Model,
    household: str,
    trip_id: str,
    contributor: str,
    display_name: str,
    count: int,
) -> CuratorResult:
    """W6: a new contributor landed in an existing trip. Refresh the story with credits and
    tell the room. Clock alignment and re-clustering already ran in the ingest pipeline."""
    tools.push_event(
        household,
        {
            "type": "contribution.received",
            "trip": trip_id,
            "contributor": contributor,
            "display_name": display_name,
            "count": count,
        },
    )
    return trip_curator(tools, model, household, trip_id)


def summarize_trip(trip: Trip) -> str:
    """One line for voice answers on Alexa+."""
    days = len(trip.days)
    who = ", ".join(sorted(trip.contributor_names.values())) or "you"
    photos = sum(len(d["media"]) for d in trip.days)
    return f"{trip.name}: {days} day{'s' if days != 1 else ''}, {photos} photos from {who}."
