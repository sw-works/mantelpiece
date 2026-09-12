"""Request handling for the MemoryLane custom skill.

Intents (skill/en-US.json): PlayTripIntent {trip}, ListTripsIntent, and the AMAZON
built-ins. Every path needs account linking; without a token the reply is a LinkAccount
card. Screens get the APL slideshow; speakers get the trip's story read aloud.
"""

from __future__ import annotations

import os
import re
from typing import Any

from .apl import SECONDS_PER_SLIDE, autopage_directive, render_directive
from .backend import Backend, BackendError, HttpBackend

MAX_SLIDES = 24
STOP_WORDS = {"the", "a", "an", "of", "in", "to", "my", "our", "trip", "trips", "photos"}
ON_TV = re.compile(r"\b(on|to)\s+(the\s+|my\s+|our\s+)?(tv|television|fire tv|big screen)\b", re.I)
LATEST = {"latest", "last", "newest", "recent", "most recent", "new"}


def handle(envelope: dict[str, Any], backend: Backend, skill_id: str | None = None) -> dict:
    """Answer one Alexa request envelope."""
    app_id = (
        envelope.get("context", {}).get("System", {}).get("application", {}).get("applicationId")
    )
    if skill_id and app_id != skill_id:
        return _speak("This request did not come from the MemoryLane skill.", end=True)
    request = envelope.get("request", {})
    kind = request.get("type")
    if kind == "SessionEndedRequest":
        return {"version": "1.0", "response": {}}
    intent = request.get("intent", {}).get("name") if kind == "IntentRequest" else None
    if intent in {"AMAZON.StopIntent", "AMAZON.CancelIntent", "AMAZON.NoIntent"}:
        return _speak("Goodbye.", end=True)
    if intent == "AMAZON.HelpIntent":
        return _speak(
            "Say play, then the name of a trip, or play the latest trip. "
            "Say list my trips to hear them all.",
            reprompt="Which trip would you like?",
        )

    token = envelope.get("context", {}).get("System", {}).get("user", {}).get("accessToken")
    if not token:
        return _link_account()
    try:
        session = backend.session(token)
    except BackendError as e:
        if e.status in (401, 403):
            return _link_account()
        return _speak(
            "MemoryLane is not answering right now. Please try again in a moment.", end=True
        )
    owner = session["token"]
    trips = sorted(backend.trips(owner), key=lambda t: t.get("start", ""), reverse=True)

    if kind == "LaunchRequest" or intent in {"ListTripsIntent", "AMAZON.FallbackIntent"}:
        return _list(trips, session.get("household_name"))
    if intent in {"PlayTripIntent", "PlayOnTvIntent", "AMAZON.YesIntent"}:
        raw = _slot(request, "trip")
        on_tv = intent == "PlayOnTvIntent" or bool(raw and ON_TV.search(raw))
        query = ON_TV.sub("", raw).strip() if raw else raw
        trip = find_trip(trips, query)
        if trip is None:
            if not trips:
                return _speak("There are no trips yet. Add photos from the TV first.", end=True)
            return _speak(
                f"I could not find a trip called {query}. " + _names_sentence(trips),
                reprompt="Which trip would you like?",
            )
        if on_tv:
            return _play_on_tv(backend, owner, trip)
        return _play(backend, owner, trip, supports_apl(envelope))
    return _speak("Say play, then the name of a trip.", reprompt="Which trip would you like?")


def supports_apl(envelope: dict[str, Any]) -> bool:
    interfaces = (
        envelope.get("context", {})
        .get("System", {})
        .get("device", {})
        .get("supportedInterfaces", {})
    )
    return "Alexa.Presentation.APL" in interfaces


def find_trip(trips: list[dict[str, Any]], query: str | None) -> dict[str, Any] | None:
    """Newest trip for an empty or "latest" query, else the best word overlap with a name."""
    if not trips:
        return None
    q = _words(query or "")
    if not q or " ".join(q) in LATEST or set(q) <= LATEST:
        return trips[0]
    best: tuple[int, dict[str, Any]] | None = None
    for trip in trips:
        name = _words(trip.get("name", ""))
        score = sum(1 for w in q if w in name)
        if score and (best is None or score > best[0]):
            best = (score, trip)
    return best[1] if best else None


def slides_for(backend: Backend, token: str, trip: dict[str, Any]) -> list[dict[str, str]]:
    """Highlights first, then each day's photos in order, capped at MAX_SLIDES."""
    order: list[tuple[str, str]] = []
    seen: set[str] = set()
    for m in trip.get("highlights", []):
        if m not in seen:
            seen.add(m)
            order.append((m, "Highlights"))
    for day in trip.get("days", []):
        caption = day.get("title") or f"Day {day.get('index', '')}".strip()
        for m in day.get("media", []):
            if m not in seen:
                seen.add(m)
                order.append((m, caption))
        if len(order) >= MAX_SLIDES:
            break
    slides = []
    for media_id, caption in order[:MAX_SLIDES]:
        try:
            url = backend.media_url(token, media_id, "tv")
        except BackendError:
            continue
        slides.append({"url": url, "title": trip.get("name", "MemoryLane"), "caption": caption})
    return slides


def story_for(trip: dict[str, Any]) -> str:
    narrative = (trip.get("narrative") or {}).get("text")
    if narrative:
        return narrative
    titles = [d.get("title") or f"Day {d.get('index')}" for d in trip.get("days", [])]
    return ". ".join(titles) + "." if titles else "No story yet."


def _play(backend: Backend, token: str, summary: dict[str, Any], apl: bool) -> dict:
    try:
        trip = backend.trip(token, summary["id"])
    except BackendError:
        return _speak("I could not open that trip.", end=True)
    name = trip.get("name") or summary.get("name") or "the trip"
    if not apl:
        return _speak(f"{name}. {story_for(trip)}", end=True)
    slides = slides_for(backend, token, trip)
    if not slides:
        return _speak(f"{name} has no photos ready yet.", end=True)
    n = len(slides)
    minutes = max(1, round(n * SECONDS_PER_SLIDE / 60))
    photos = f"{n} photo{'s' if n != 1 else ''}"
    length = f"about {minutes} minute{'s' if minutes != 1 else ''}"
    speech = f"Playing {name}: {photos}, {length}."
    return {
        "version": "1.0",
        "response": {
            "outputSpeech": {"type": "PlainText", "text": speech},
            "directives": [render_directive(slides), autopage_directive(n)],
            "shouldEndSession": True,
        },
    }


def _play_on_tv(backend: Backend, token: str, summary: dict[str, Any]) -> dict:
    """Send the trip to the household's Fire TV over the feed instead of this screen."""
    try:
        out = backend.play_on_tv(token, summary["id"])
    except BackendError:
        return _speak("I could not reach your TV.", end=True)
    name = out.get("name") or summary.get("name") or "the trip"
    return _speak(f"Playing {name} on your TV.", end=True)


def _list(trips: list[dict[str, Any]], household_name: str | None) -> dict:
    where = f" in {household_name}" if household_name else ""
    if not trips:
        return _speak(f"There are no trips{where} yet. Add photos from the TV first.", end=True)
    n = len(trips)
    return _speak(
        f"You have {n} trip{'s' if n != 1 else ''}{where}. "
        f"{_names_sentence(trips)} Which one shall I play?",
        reprompt="Say play, then the name of a trip.",
    )


def _names_sentence(trips: list[dict[str, Any]], limit: int = 5) -> str:
    names = [t.get("name", "") for t in trips[:limit] if t.get("name")]
    if not names:
        return ""
    more = f", and {len(trips) - limit} more" if len(trips) > limit else ""
    return f"The latest are {', '.join(names)}{more}."


def _slot(request: dict[str, Any], name: str) -> str | None:
    slot = request.get("intent", {}).get("slots", {}).get(name, {})
    value = slot.get("value")
    return value.strip() if isinstance(value, str) and value.strip() else None


def _words(text: str) -> list[str]:
    return [w for w in re.findall(r"[a-z0-9]+", text.lower()) if w not in STOP_WORDS]


def _speak(text: str, *, end: bool = False, reprompt: str | None = None) -> dict:
    response: dict[str, Any] = {
        "outputSpeech": {"type": "PlainText", "text": text},
        "shouldEndSession": end,
    }
    if reprompt and not end:
        response["reprompt"] = {"outputSpeech": {"type": "PlainText", "text": reprompt}}
    return {"version": "1.0", "response": response}


def _link_account() -> dict:
    return {
        "version": "1.0",
        "response": {
            "outputSpeech": {
                "type": "PlainText",
                "text": (
                    "Please link your Amazon account to MemoryLane in the Alexa app, "
                    "then ask again."
                ),
            },
            "card": {"type": "LinkAccount"},
            "shouldEndSession": True,
        },
    }


_backend: HttpBackend | None = None


def lambda_handler(event: dict[str, Any], _context: Any = None) -> dict:
    global _backend
    if _backend is None:
        _backend = HttpBackend(os.environ["API_BASE"])
    return handle(event, _backend, os.environ.get("SKILL_ID") or None)
