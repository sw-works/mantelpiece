"""Tools over AgentCore Gateway (MCP, Streamable HTTP).

Not exercised locally: there is no Gateway to talk to on the development machine. The
shape mirrors :class:`memorylane_agents.tools.Tools`; each method is one MCP tool call whose
target is the corresponding API Lambda.
"""

from __future__ import annotations

from typing import Any

from .tools import DedupGroup, Trip


class GatewayTools:
    def __init__(self, url: str, token_provider: str) -> None:
        self.url = url
        self.token_provider = token_provider
        self._client: Any = None

    def _call(self, tool: str, **arguments: Any) -> Any:
        if self._client is None:
            from mcp.client.streamable_http import (
                streamablehttp_client,  # type: ignore[import-not-found]
            )
            from strands.tools.mcp import MCPClient  # type: ignore[import-not-found]

            self._client = MCPClient(lambda: streamablehttp_client(self.url))
            self._client.start()
        result = self._client.call_tool_sync(tool_use_id=tool, name=tool, arguments=arguments)
        return result["content"][0].get("json") or result["content"][0].get("text")

    def list_trips(self, household: str) -> list[Trip]:
        return [Trip(**t) for t in self._call("list_trips", household=household)]

    def get_trip(self, household: str, trip_id: str) -> Trip:
        return Trip(**self._call("get_trip", household=household, trip_id=trip_id))

    def search_media(self, household: str, query: str, trip_id: str | None = None) -> list[str]:
        return list(self._call("search_media", household=household, query=query, trip_id=trip_id))

    def create_playlist(self, household: str, name: str, media: list[str], source: str) -> str:
        return str(
            self._call(
                "create_playlist", household=household, name=name, media=media, source=source
            )
        )

    def save_narrative(self, household: str, trip_id: str, text: str, model_id: str) -> None:
        self._call(
            "save_narrative", household=household, trip_id=trip_id, text=text, model_id=model_id
        )

    def list_duplicates(self, household: str, trip_id: str | None = None) -> list[DedupGroup]:
        return [
            DedupGroup(**g)
            for g in self._call("list_duplicates", household=household, trip_id=trip_id)
        ]

    def request_confirmation(self, household: str, action: dict[str, Any]) -> str | None:
        return self._call("request_confirmation", household=household, action=action)

    def apply_dedup(self, household: str, group_ids: list[str], confirmation: str) -> int:
        return int(
            self._call(
                "apply_dedup", household=household, group_ids=group_ids, confirmation=confirmation
            )
        )

    def share_album(self, household: str, media: list[str], name: str, confirmation: str) -> str:
        return str(
            self._call(
                "share_album",
                household=household,
                media=media,
                name=name,
                confirmation=confirmation,
            )
        )

    def push_event(self, household: str, event: dict[str, Any]) -> None:
        self._call("push_event", household=household, event=event)

    def read_memory(self, household: str) -> dict[str, Any]:
        return dict(self._call("read_memory", household=household) or {})

    def write_memory(self, household: str, key: str, value: Any) -> None:
        self._call("write_memory", household=household, key=key, value=value)
