"""Map EventBridge events onto workflows (F-503)."""

from __future__ import annotations

from typing import Any

from . import workflows
from .model import Model
from .tools import Tools


def handle_event(tools: Tools, model: Model, detail_type: str, detail: dict[str, Any]) -> Any:
    household = detail["household"]
    if detail_type == "trip.detected":
        return workflows.trip_curator(tools, model, household, detail["trip"])
    if detail_type == "contribution.received" and detail.get("trip"):
        return workflows.trip_merge(
            tools,
            model,
            household,
            detail["trip"],
            detail["contributor"],
            detail.get("display_name", detail["contributor"]),
            int(detail.get("count", 0)),
        )
    if detail_type == "dedup.proposal":
        return workflows.dedup_steward(tools, household, detail.get("trip"))
    if detail_type == "cleanup.candidates":
        return workflows.quick_cleanup(tools, household, detail.get("trip"))
    if detail_type in {"nightly", "nightly.tick"}:
        return workflows.nightly(tools, household, detail.get("feedback"))
    return None
