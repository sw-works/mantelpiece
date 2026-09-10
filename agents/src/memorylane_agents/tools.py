"""Tools the Concierge calls.

In production these are the API Lambdas exposed through AgentCore Gateway as MCP tools.
The agent code only depends on this protocol, so workflows run in tests against
:class:`InMemoryTools` and in AgentCore against :class:`GatewayTools`.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Protocol


class ConfirmationRequired(Exception):
    """Raised when an irreversible tool is called without a valid confirmation token."""


@dataclass
class Trip:
    id: str
    name: str
    start: str
    end: str
    days: list[dict[str, Any]]
    highlights: list[str]
    contributors: list[str]
    narrative: dict[str, Any] | None = None
    landmarks: list[str] = field(default_factory=list)
    contributor_names: dict[str, str] = field(default_factory=dict)


@dataclass
class DedupGroup:
    id: str
    keeper: str
    members: list[str]


@dataclass
class NowShowing:
    """What a TV is showing right now (research R3). ``None`` when nothing is fresh."""

    media: str
    trip: str | None
    day: int | None
    paused: bool
    at: str


@dataclass
class PhotoAnswer:
    """``describe_photo`` result: the answer comes from the stored description and the
    photo's metadata, never from a fresh model call on the Alexa+ path (research R8)."""

    answer: str
    landmark: str | None
    taken_by: str
    when: str
    related: list[str] = field(default_factory=list)


@dataclass
class CleanupCandidate:
    media: str
    reason: str  # screenshot | document | receipt | blurry
    trip: str | None = None


@dataclass
class Memories:
    """Nightly resurfacing sets (F-701 to F-703)."""

    date: str
    on_this_day: list[str] = field(default_factory=list)
    then_and_now: list[dict[str, Any]] = field(default_factory=list)
    best_of: list[str] = field(default_factory=list)


class Tools(Protocol):
    def list_trips(self, household: str) -> list[Trip]: ...
    def get_trip(self, household: str, trip_id: str) -> Trip: ...
    def search_media(self, household: str, query: str, trip_id: str | None = None) -> list[str]: ...
    def create_playlist(self, household: str, name: str, media: list[str], source: str) -> str: ...
    def save_narrative(self, household: str, trip_id: str, text: str, model_id: str) -> None: ...
    def list_duplicates(self, household: str, trip_id: str | None = None) -> list[DedupGroup]: ...
    def request_confirmation(self, household: str, action: dict[str, Any]) -> str | None:
        """Open Review Changes on the TV. Returns the token once the user confirms."""
        ...

    def apply_dedup(self, household: str, group_ids: list[str], confirmation: str) -> int: ...
    def share_album(
        self, household: str, media: list[str], name: str, confirmation: str
    ) -> str: ...
    def push_event(self, household: str, event: dict[str, Any]) -> None: ...
    def read_memory(self, household: str) -> dict[str, Any]: ...
    def write_memory(self, household: str, key: str, value: Any) -> None: ...
    def get_now_showing(self, household: str, tv: str | None = None) -> NowShowing | None: ...
    def describe_photo(self, household: str, media_id: str, question: str) -> PhotoAnswer: ...
    def list_cleanup_candidates(
        self, household: str, trip_id: str | None = None
    ) -> list[CleanupCandidate]: ...
    def memories_for(self, household: str, date: str | None = None) -> Memories: ...
    def apply_cleanup(self, household: str, media_ids: list[str], confirmation: str) -> int: ...


@dataclass
class InMemoryTools:
    """Scripted backend for tests and local runs. Records every call in ``log``."""

    trips: dict[str, Trip] = field(default_factory=dict)
    media_index: dict[str, list[str]] = field(default_factory=dict)
    duplicates: list[DedupGroup] = field(default_factory=list)
    confirmations: list[str | None] = field(default_factory=list)
    memory: dict[str, Any] = field(default_factory=dict)
    log: list[str] = field(default_factory=list)
    playlists: dict[str, list[str]] = field(default_factory=dict)
    events: list[dict[str, Any]] = field(default_factory=list)
    now_showing: dict[str, NowShowing] = field(default_factory=dict)
    descriptions: dict[str, PhotoAnswer] = field(default_factory=dict)
    cleanup: list[CleanupCandidate] = field(default_factory=list)
    memories: dict[str, Memories] = field(default_factory=dict)
    removed: list[str] = field(default_factory=list)

    def list_trips(self, household: str) -> list[Trip]:
        self.log.append(f"list_trips {household}")
        return list(self.trips.values())

    def get_trip(self, household: str, trip_id: str) -> Trip:
        self.log.append(f"get_trip {trip_id}")
        return self.trips[trip_id]

    def search_media(self, household: str, query: str, trip_id: str | None = None) -> list[str]:
        self.log.append(f"search_media {query!r} trip={trip_id}")
        hits = [m for key, ms in self.media_index.items() if key in query.lower() for m in ms]
        if trip_id:
            allowed = {m for d in self.trips[trip_id].days for m in d["media"]}
            hits = [m for m in hits if m in allowed]
        return hits

    def create_playlist(self, household: str, name: str, media: list[str], source: str) -> str:
        pid = f"pl-{len(self.playlists) + 1}"
        self.playlists[pid] = media
        self.log.append(f"create_playlist {name!r} {len(media)}")
        return pid

    def save_narrative(self, household: str, trip_id: str, text: str, model_id: str) -> None:
        self.trips[trip_id].narrative = {"text": text, "model_id": model_id}
        self.log.append(f"save_narrative {trip_id}")

    def list_duplicates(self, household: str, trip_id: str | None = None) -> list[DedupGroup]:
        self.log.append(f"list_duplicates trip={trip_id}")
        return list(self.duplicates)

    def request_confirmation(self, household: str, action: dict[str, Any]) -> str | None:
        self.log.append(f"request_confirmation {action['action']}")
        return self.confirmations.pop(0) if self.confirmations else None

    def apply_dedup(self, household: str, group_ids: list[str], confirmation: str) -> int:
        if not confirmation:
            raise ConfirmationRequired("apply_dedup")
        self.log.append(f"apply_dedup {len(group_ids)} {confirmation}")
        removed = sum(len(g.members) - 1 for g in self.duplicates if g.id in group_ids)
        self.duplicates = [g for g in self.duplicates if g.id not in group_ids]
        return removed

    def share_album(self, household: str, media: list[str], name: str, confirmation: str) -> str:
        if not confirmation:
            raise ConfirmationRequired("share_album")
        self.log.append(f"share_album {name!r} {len(media)} {confirmation}")
        return f"https://share.test/{name.lower().replace(' ', '-')}"

    def push_event(self, household: str, event: dict[str, Any]) -> None:
        self.events.append(event)
        self.log.append(f"push_event {event['type']}")

    def read_memory(self, household: str) -> dict[str, Any]:
        return dict(self.memory)

    def write_memory(self, household: str, key: str, value: Any) -> None:
        self.memory[key] = value
        self.log.append(f"write_memory {key}")

    def get_now_showing(self, household: str, tv: str | None = None) -> NowShowing | None:
        self.log.append(f"get_now_showing tv={tv}")
        if tv is not None:
            return self.now_showing.get(tv)
        if not self.now_showing:
            return None
        return max(self.now_showing.values(), key=lambda p: p.at)

    def describe_photo(self, household: str, media_id: str, question: str) -> PhotoAnswer:
        self.log.append(f"describe_photo {media_id} {question!r}")
        if media_id not in self.descriptions:
            raise KeyError(media_id)
        return self.descriptions[media_id]

    def list_cleanup_candidates(
        self, household: str, trip_id: str | None = None
    ) -> list[CleanupCandidate]:
        self.log.append(f"list_cleanup_candidates trip={trip_id}")
        return [c for c in self.cleanup if trip_id is None or c.trip == trip_id]

    def memories_for(self, household: str, date: str | None = None) -> Memories:
        self.log.append(f"memories_for {date}")
        key = date or "today"
        return self.memories.get(key, Memories(date=key))

    def apply_cleanup(self, household: str, media_ids: list[str], confirmation: str) -> int:
        if not confirmation:
            raise ConfirmationRequired("apply_cleanup")
        self.log.append(f"apply_cleanup {len(media_ids)} {confirmation}")
        before = len(self.cleanup)
        self.cleanup = [c for c in self.cleanup if c.media not in media_ids]
        self.removed.extend(media_ids)
        return before - len(self.cleanup)
