"""The in-memory tool fakes behave like the contract in contracts/tools.md."""

import pytest

from mantelpiece_agents.tools import (
    CleanupCandidate,
    ConfirmationRequired,
    InMemoryTools,
    Memories,
    NowShowing,
    PhotoAnswer,
)


def test_now_showing_prefers_the_named_tv_then_the_freshest() -> None:
    t = InMemoryTools()
    assert t.get_now_showing("h") is None
    t.now_showing["tv-1"] = NowShowing("m1", "kyoto", 2, True, "2026-09-08T10:00:00Z")
    t.now_showing["tv-2"] = NowShowing("m9", "kyoto", 3, False, "2026-09-08T10:05:00Z")
    assert t.get_now_showing("h").media == "m9"
    assert t.get_now_showing("h", tv="tv-1").media == "m1"
    assert t.get_now_showing("h", tv="tv-3") is None


def test_describe_photo_answers_from_stored_descriptions_only() -> None:
    t = InMemoryTools()
    t.descriptions["m1"] = PhotoAnswer(
        answer="The Great Buddha at Todai-ji, Nara.",
        landmark="Todai-ji",
        taken_by="Maya",
        when="2026-04-04",
        related=["m2"],
    )
    a = t.describe_photo("h", "m1", "what's that statue")
    assert a.landmark == "Todai-ji" and a.related == ["m2"]
    with pytest.raises(KeyError):
        t.describe_photo("h", "m404", "?")


def test_cleanup_needs_a_confirmation_and_filters_by_trip() -> None:
    t = InMemoryTools()
    t.cleanup = [
        CleanupCandidate("m1", "screenshot", "kyoto"),
        CleanupCandidate("m2", "receipt", "kyoto"),
        CleanupCandidate("m3", "blurry", "lisbon"),
    ]
    assert [c.media for c in t.list_cleanup_candidates("h", "kyoto")] == ["m1", "m2"]
    assert len(t.list_cleanup_candidates("h")) == 3
    with pytest.raises(ConfirmationRequired):
        t.apply_cleanup("h", ["m1"], "")
    assert t.apply_cleanup("h", ["m1", "m3"], "conf-1") == 2
    assert t.removed == ["m1", "m3"]
    assert [c.media for c in t.cleanup] == ["m2"]


def test_memories_default_to_empty_sets() -> None:
    t = InMemoryTools()
    t.memories["2026-09-08"] = Memories(
        date="2026-09-08", on_this_day=["m1"], then_and_now=[{"then": "m1", "now": "m2"}]
    )
    assert t.memories_for("h", "2026-09-08").on_this_day == ["m1"]
    assert t.memories_for("h").on_this_day == []
