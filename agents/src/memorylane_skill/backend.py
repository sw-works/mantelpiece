"""The MemoryLane API as the skill sees it. `HttpBackend` speaks to the deployed API with
the account-linked Cognito token, trading it once per container for an owner session;
`InMemoryBackend` answers from fixtures for tests."""

from __future__ import annotations

import json
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from typing import Any, Protocol


class BackendError(Exception):
    def __init__(self, status: int, message: str) -> None:
        super().__init__(message)
        self.status = status


class Backend(Protocol):
    def session(self, access_token: str) -> dict[str, Any]:
        """`{household, household_name, token}` for a Cognito access token."""

    def trips(self, token: str) -> list[dict[str, Any]]: ...

    def trip(self, token: str, trip_id: str) -> dict[str, Any]: ...

    def media_url(self, token: str, media_id: str, rendition: str = "tv") -> str: ...


@dataclass
class InMemoryBackend:
    trips_by_household: dict[str, list[dict[str, Any]]] = field(default_factory=dict)
    details: dict[str, dict[str, Any]] = field(default_factory=dict)
    names: dict[str, str] = field(default_factory=dict)
    calls: list[str] = field(default_factory=list)
    reject: set[str] = field(default_factory=set)

    def session(self, access_token: str) -> dict[str, Any]:
        self.calls.append(f"session {access_token}")
        if access_token in self.reject:
            raise BackendError(401, "token rejected")
        household = access_token.split(":", 1)[-1]
        return {
            "household": household,
            "household_name": self.names.get(household),
            "token": f"session-{household}",
        }

    def trips(self, token: str) -> list[dict[str, Any]]:
        self.calls.append("trips")
        return list(self.trips_by_household.get(token.removeprefix("session-"), []))

    def trip(self, token: str, trip_id: str) -> dict[str, Any]:
        self.calls.append(f"trip {trip_id}")
        if trip_id not in self.details:
            raise BackendError(404, "no such trip")
        return self.details[trip_id]

    def media_url(self, token: str, media_id: str, rendition: str = "tv") -> str:
        self.calls.append(f"url {media_id} {rendition}")
        return f"https://cdn.test/{media_id}-{rendition}.jpg"


class HttpBackend:
    """urllib client; sessions cached per access token for the container's lifetime."""

    def __init__(self, base: str, session_ttl_s: float = 20 * 60) -> None:
        self.base = base.rstrip("/")
        self.session_ttl_s = session_ttl_s
        self._sessions: dict[str, tuple[float, dict[str, Any]]] = {}

    def _call(
        self,
        method: str,
        path: str,
        *,
        bearer: str | None = None,
        body: dict[str, Any] | None = None,
    ) -> Any:
        data = json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(f"{self.base}{path}", data=data, method=method)
        req.add_header("accept", "application/json")
        if data is not None:
            req.add_header("content-type", "application/json")
        if bearer:
            req.add_header("authorization", f"Bearer {bearer}")
        try:
            with urllib.request.urlopen(req, timeout=8) as res:  # noqa: S310
                raw = res.read()
        except urllib.error.HTTPError as e:
            try:
                message = json.loads(e.read().decode() or "{}").get("error") or str(e)
            except ValueError:
                message = str(e)
            raise BackendError(e.code, message) from e
        return json.loads(raw.decode() or "null")

    def session(self, access_token: str) -> dict[str, Any]:
        cached = self._sessions.get(access_token)
        now = time.monotonic()
        if cached and cached[0] > now:
            return cached[1]
        session = self._call("POST", "/session/amazon", bearer=access_token)
        self._sessions[access_token] = (now + self.session_ttl_s, session)
        return session

    def trips(self, token: str) -> list[dict[str, Any]]:
        return self._call("GET", "/trips", bearer=token)

    def trip(self, token: str, trip_id: str) -> dict[str, Any]:
        return self._call("GET", f"/trips/{trip_id}", bearer=token)

    def media_url(self, token: str, media_id: str, rendition: str = "tv") -> str:
        out = self._call("GET", f"/media/{media_id}/url?rendition={rendition}", bearer=token)
        return out["url"]
