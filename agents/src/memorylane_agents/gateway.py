"""Tools over AgentCore Gateway (MCP, Streamable HTTP).

Each method is one MCP ``tools/call`` whose target is the API Lambda. The Gateway is
stateless, so a call is a single JSON-RPC POST: no session, no SDK, standard library only.
The agent signs in as itself with Cognito client credentials (scope ``memorylane/tools``);
the token is cached until shortly before it expires.

The HTTP transport is injectable, which is how the tests drive this without a Gateway.
"""

from __future__ import annotations

import base64
import json
import time
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable
from dataclasses import fields
from typing import Any

from .tools import CleanupCandidate, DedupGroup, Memories, NowShowing, PhotoAnswer, Trip
from .tracing import ATTR_HOUSEHOLD, ATTR_TOOL, tracer

MCP_VERSION = "2025-11-25"

# (url, headers, body) -> (status, response text)
Transport = Callable[[str, dict[str, str], bytes], tuple[int, str]]


class ToolError(RuntimeError):
    """The Gateway or the tool refused the call."""


def _urllib_transport(url: str, headers: dict[str, str], body: bytes) -> tuple[int, str]:
    request = urllib.request.Request(url, data=body, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.status, response.read().decode()
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()


def _only(cls: type, data: dict[str, Any]) -> Any:
    """Build a dataclass from an API object that carries more fields than the agent uses."""
    names = {f.name for f in fields(cls)}
    return cls(**{k: v for k, v in data.items() if k in names})


def _message(text: str) -> dict[str, Any]:
    """A JSON-RPC message from a plain JSON body or from a server-sent event stream."""
    text = text.strip()
    if text.startswith("{"):
        return json.loads(text)
    for line in text.splitlines():
        if line.startswith("data:"):
            return json.loads(line[5:].strip())
    raise ToolError(f"unreadable Gateway response: {text[:200]}")


class GatewayTools:
    def __init__(
        self,
        url: str,
        token_url: str,
        client_id: str,
        client_secret: str,
        scope: str = "memorylane/tools",
        target: str = "api",
        transport: Transport = _urllib_transport,
        clock: Callable[[], float] = time.time,
    ) -> None:
        self.url = url
        self.token_url = token_url
        self.client_id = client_id
        self.client_secret = client_secret
        self.scope = scope
        self.target = target
        self._transport = transport
        self._clock = clock
        self._token: str | None = None
        self._token_until = 0.0
        self._next_id = 0

    def _bearer(self) -> str:
        if self._token and self._clock() < self._token_until:
            return self._token
        basic = base64.b64encode(f"{self.client_id}:{self.client_secret}".encode()).decode()
        form = urllib.parse.urlencode({"grant_type": "client_credentials", "scope": self.scope})
        status, text = self._transport(
            self.token_url,
            {
                "authorization": f"Basic {basic}",
                "content-type": "application/x-www-form-urlencoded",
            },
            form.encode(),
        )
        if status != 200:
            raise ToolError(f"sign-in to the Gateway failed: {status} {text[:200]}")
        granted = json.loads(text)
        self._token = str(granted["access_token"])
        self._token_until = self._clock() + float(granted.get("expires_in", 3600)) - 60
        return self._token

    def _call(self, tool: str, **arguments: Any) -> Any:
        arguments = {k: v for k, v in arguments.items() if v is not None}
        # F-506: every tool call is a span under the run's span, tagged with the household.
        attrs = {ATTR_TOOL: tool, ATTR_HOUSEHOLD: arguments.get("household")}
        with tracer().span(f"tool.{tool}", attrs):
            self._next_id += 1
            body = {
                "jsonrpc": "2.0",
                "id": self._next_id,
                "method": "tools/call",
                "params": {"name": f"{self.target}___{tool}", "arguments": arguments},
            }
            status, text = self._transport(
                self.url,
                {
                    "authorization": f"Bearer {self._bearer()}",
                    "content-type": "application/json",
                    "accept": "application/json, text/event-stream",
                    "mcp-protocol-version": MCP_VERSION,
                },
                json.dumps(body).encode(),
            )
        if status == 401:
            self._token = None
        if status >= 300:
            raise ToolError(f"{tool}: Gateway answered {status} {text[:200]}")
        message = _message(text)
        if "error" in message:
            raise ToolError(f"{tool}: {message['error'].get('message', message['error'])}")
        result = message.get("result", {})
        content = result.get("content") or [{}]
        raw = content[0].get("text", "")
        try:
            value = json.loads(raw) if raw else None
        except json.JSONDecodeError:
            value = raw
        if result.get("isError"):
            raise ToolError(f"{tool}: {raw[:200]}")
        # The API Lambda reports its own refusals as {"error": ...}.
        if isinstance(value, dict) and set(value) == {"error"}:
            raise ToolError(f"{tool}: {value['error']}")
        return value

    def list_trips(self, household: str) -> list[Trip]:
        return [_only(Trip, t) for t in self._call("list_trips", household=household)]

    def get_trip(self, household: str, trip_id: str) -> Trip:
        return _only(Trip, self._call("get_trip", household=household, trip_id=trip_id))

    def search_media(self, household: str, query: str, trip_id: str | None = None) -> list[str]:
        found = self._call("search_media", household=household, query=query, trip_id=trip_id)
        hits = found.get("results", []) if isinstance(found, dict) else found or []
        return [str(h["media"]) if isinstance(h, dict) else str(h) for h in hits]

    def create_playlist(self, household: str, name: str, media: list[str], source: str) -> str:
        made = self._call(
            "create_playlist", household=household, name=name, media=media, source=source
        )
        return str(made["id"]) if isinstance(made, dict) else str(made)

    def save_narrative(
        self,
        household: str,
        trip_id: str,
        text: str,
        model_id: str,
        stories: list[str] | None = None,
    ) -> None:
        self._call(
            "save_narrative",
            household=household,
            trip_id=trip_id,
            text=text,
            model_id=model_id,
            stories=stories,
        )

    def withhold_narrative(self, household: str, trip_id: str, model_id: str) -> None:
        self._call(
            "save_narrative",
            household=household,
            trip_id=trip_id,
            text="",
            model_id=model_id,
            withhold=True,
        )

    def list_duplicates(self, household: str, trip_id: str | None = None) -> list[DedupGroup]:
        return [
            DedupGroup(**g)
            for g in self._call("list_duplicates", household=household, trip_id=trip_id)
        ]

    def request_confirmation(self, household: str, action: dict[str, Any]) -> str | None:
        # The Gateway's schema types structured arguments as JSON text.
        return self._call("request_confirmation", household=household, action=json.dumps(action))

    def apply_dedup(self, household: str, group_ids: list[str], confirmation: str) -> int:
        return int(
            self._call(
                "apply_dedup", household=household, group_ids=group_ids, confirmation=confirmation
            )
        )

    def save_landmark_trivia(self, household: str, landmark: str, lines: list[str]) -> list[str]:
        try:
            self._call("save_landmark_trivia", household=household, landmark=landmark, lines=lines)
        except ToolError as refused:
            # 422 from the fact check: the reasons are what the retry needs.
            return [str(refused)]
        return []

    def get_trip_code(self, household: str, trip_id: str) -> str:
        issued = self._call("get_trip_code", household=household, trip_id=trip_id)
        return str(issued["code"]) if isinstance(issued, dict) else str(issued)

    def share_album(
        self,
        household: str,
        media: list[str],
        name: str,
        confirmation: str,
        playlist: str | None = None,
        email: str | None = None,
    ) -> str:
        return str(
            self._call(
                "share_album",
                household=household,
                media=media,
                name=name,
                confirmation=confirmation,
                playlist=playlist,
                email=email,
            )
        )

    def push_event(self, household: str, event: dict[str, Any]) -> None:
        self._call("push_event", household=household, event=json.dumps(event))

    def read_memory(self, household: str) -> dict[str, Any]:
        return dict(self._call("read_memory", household=household) or {})

    def write_memory(self, household: str, key: str, value: Any) -> None:
        self._call("write_memory", household=household, key=key, value=json.dumps(value))

    def get_now_showing(self, household: str, tv: str | None = None) -> NowShowing | None:
        out = self._call("get_now_showing", household=household, tv=tv)
        return NowShowing(**out) if out else None

    def describe_photo(self, household: str, media_id: str, question: str) -> PhotoAnswer:
        return PhotoAnswer(
            **self._call(
                "describe_photo", household=household, media_id=media_id, question=question
            )
        )

    def list_cleanup_candidates(
        self, household: str, trip_id: str | None = None
    ) -> list[CleanupCandidate]:
        return [
            CleanupCandidate(**c)
            for c in self._call("list_cleanup_candidates", household=household, trip_id=trip_id)
        ]

    def memories_for(self, household: str, date: str | None = None) -> Memories:
        return Memories(**self._call("memories_for", household=household, date=date))

    def get_household(self, household: str) -> dict[str, Any]:
        return dict(self._call("get_household", household=household) or {})

    def synthesize_narration(self, household: str, trip_id: str, day: int) -> str:
        made = self._call("synthesize_narration", household=household, trip_id=trip_id, day=day)
        return str(made["audio_key"]) if isinstance(made, dict) else str(made)

    def purge_expired(self, household: str) -> int:
        return int(self._call("purge_expired", household=household) or 0)

    def apply_cleanup(self, household: str, media_ids: list[str], confirmation: str) -> int:
        return int(
            self._call(
                "apply_cleanup",
                household=household,
                media_ids=media_ids,
                confirmation=confirmation,
            )
        )
