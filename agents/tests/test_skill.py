from memorylane_skill.apl import PAGER_ID, TOKEN
from memorylane_skill.backend import InMemoryBackend
from memorylane_skill.handler import find_trip, handle, slides_for, story_for

SKYE = {
    "id": "skye",
    "name": "Skye and Lochalsh, April 2025",
    "start": "2025-04-04T00:00:00Z",
    "end": "2025-04-06T00:00:00Z",
    "highlights": ["m3"],
}
KYOTO = {
    "id": "kyoto",
    "name": "Kyoto, April 2026",
    "start": "2026-04-03T00:00:00Z",
    "end": "2026-04-05T00:00:00Z",
    "highlights": [],
}
KYOTO_DETAIL = {
    **KYOTO,
    "narrative": {"text": "Three days in Kyoto with the kids."},
    "days": [
        {"index": 1, "title": "Day 1: Kinkaku-ji", "media": ["m1", "m2"]},
        {"index": 2, "title": None, "media": ["m3"]},
    ],
}


def backend() -> InMemoryBackend:
    b = InMemoryBackend()
    b.trips_by_household["h1"] = [SKYE, KYOTO]
    b.details["kyoto"] = KYOTO_DETAIL
    b.details["skye"] = {
        **SKYE,
        "narrative": None,
        "days": [{"index": 1, "title": "Day 1: Skye", "media": ["m3", "m4"]}],
    }
    b.names["h1"] = "Living room"
    return b


def envelope(
    kind: str,
    intent: str | None = None,
    slots: dict | None = None,
    *,
    token: str | None = "amzn:h1",
    apl: bool = True,
    app_id: str = "amzn1.ask.skill.test",
) -> dict:
    request: dict = {"type": kind, "requestId": "r1"}
    if intent:
        request["intent"] = {
            "name": intent,
            "slots": {k: {"name": k, "value": v} for k, v in (slots or {}).items()},
        }
    user = {"userId": "u1"}
    if token:
        user["accessToken"] = token
    return {
        "version": "1.0",
        "context": {
            "System": {
                "application": {"applicationId": app_id},
                "user": user,
                "device": {"supportedInterfaces": {"Alexa.Presentation.APL": {}} if apl else {}},
            }
        },
        "request": request,
    }


def speech(res: dict) -> str:
    return res["response"]["outputSpeech"]["text"]


def test_unlinked_account_gets_the_link_card():
    res = handle(envelope("LaunchRequest", token=None), backend())
    assert res["response"]["card"] == {"type": "LinkAccount"}
    assert res["response"]["shouldEndSession"] is True


def test_rejected_token_also_asks_to_link():
    b = backend()
    b.reject.add("amzn:h1")
    res = handle(envelope("LaunchRequest"), b)
    assert res["response"]["card"] == {"type": "LinkAccount"}


def test_launch_lists_trips_newest_first_and_stays_open():
    res = handle(envelope("LaunchRequest"), backend())
    assert speech(res) == (
        "You have 2 trips in Living room. The latest are Kyoto, April 2026, "
        "Skye and Lochalsh, April 2025. Which one shall I play?"
    )
    assert res["response"]["shouldEndSession"] is False
    assert "reprompt" in res["response"]


def test_play_on_a_screen_renders_the_slideshow_and_autopages():
    b = backend()
    res = handle(envelope("IntentRequest", "PlayTripIntent", {"trip": "kyoto"}), b)
    assert speech(res) == "Playing Kyoto, April 2026: 3 photos, about 1 minute."
    render, auto = res["response"]["directives"]
    assert render["type"] == "Alexa.Presentation.APL.RenderDocument"
    assert render["token"] == TOKEN
    slides = render["datasources"]["payload"]["slides"]
    assert [s["url"] for s in slides] == [
        "https://cdn.test/m1-tv.jpg",
        "https://cdn.test/m2-tv.jpg",
        "https://cdn.test/m3-tv.jpg",
    ]
    assert slides[0]["caption"] == "Day 1: Kinkaku-ji"
    assert slides[2]["caption"] == "Day 2"
    assert auto["commands"][0] == {
        "type": "AutoPage",
        "componentId": PAGER_ID,
        "count": 2,
        "duration": 8000,
        "transitionDuration": 600,
    }
    assert res["response"]["shouldEndSession"] is True
    assert "trip kyoto" in b.calls


def test_play_without_a_screen_reads_the_story():
    res = handle(
        envelope("IntentRequest", "PlayTripIntent", {"trip": "kyoto"}, apl=False), backend()
    )
    assert speech(res) == "Kyoto, April 2026. Three days in Kyoto with the kids."
    assert "directives" not in res["response"]


def test_latest_and_fuzzy_names_resolve():
    trips = sorted([SKYE, KYOTO], key=lambda t: t["start"], reverse=True)
    assert find_trip(trips, None)["id"] == "kyoto"
    assert find_trip(trips, "the latest trip")["id"] == "kyoto"
    assert find_trip(trips, "skye")["id"] == "skye"
    assert find_trip(trips, "lochalsh photos")["id"] == "skye"
    assert find_trip(trips, "lisbon") is None
    assert find_trip([], "anything") is None


def test_unknown_trip_offers_the_names():
    res = handle(envelope("IntentRequest", "PlayTripIntent", {"trip": "lisbon"}), backend())
    assert speech(res).startswith("I could not find a trip called lisbon. The latest are Kyoto")
    assert res["response"]["shouldEndSession"] is False


def test_highlights_lead_the_slides_and_the_story_falls_back_to_day_titles():
    b = backend()
    slides = slides_for(b, "session-h1", b.details["skye"])
    assert [s["caption"] for s in slides] == ["Highlights", "Day 1: Skye"]
    assert story_for(b.details["skye"]) == "Day 1: Skye."


def test_wrong_skill_id_is_refused_and_stop_ends():
    res = handle(
        envelope("LaunchRequest", app_id="amzn1.ask.skill.other"),
        backend(),
        skill_id="amzn1.ask.skill.test",
    )
    assert "did not come from" in speech(res)
    stop = handle(envelope("IntentRequest", "AMAZON.StopIntent"), backend())
    assert stop["response"]["shouldEndSession"] is True
    assert handle(envelope("SessionEndedRequest"), backend()) == {"version": "1.0", "response": {}}


def test_on_the_tv_sends_the_trip_to_the_fire_tv_instead_of_the_screen():
    b = backend()
    res = handle(envelope("IntentRequest", "PlayOnTvIntent", {"trip": "skye"}), b)
    assert speech(res) == "Playing Skye and Lochalsh, April 2025 on your TV."
    assert "directives" not in res["response"]
    assert "play skye" in b.calls
    assert res["response"]["shouldEndSession"] is True

    # The phrase inside a PlayTripIntent slot works too, and the name still matches.
    b = backend()
    res = handle(envelope("IntentRequest", "PlayTripIntent", {"trip": "kyoto on the tv"}), b)
    assert speech(res) == "Playing Kyoto, April 2026 on your TV."
    assert "play kyoto" in b.calls
    assert "trip kyoto" not in b.calls
