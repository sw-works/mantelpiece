"""Spans for AgentCore Observability (F-506).

Every agent run and every Gateway tool call opens a span tagged with the household and
the workflow, so the Observability console can filter one family's activity and a
tool call can be tied back to the run that made it. OpenTelemetry is used when the
``aws-opentelemetry-distro`` that AgentCore Runtime injects is importable; otherwise a
recording tracer keeps the same shape for tests and local runs.
"""

from __future__ import annotations

from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass, field
from typing import Any

ATTR_HOUSEHOLD = "memorylane.household"
ATTR_WORKFLOW = "memorylane.workflow"
ATTR_TOOL = "memorylane.tool"
ATTR_TRIGGER = "memorylane.trigger"


@dataclass
class Span:
    name: str
    attributes: dict[str, Any] = field(default_factory=dict)
    error: str | None = None


class RecordingTracer:
    """Keeps every span in order; what tests and local runs see."""

    def __init__(self) -> None:
        self.spans: list[Span] = []

    @contextmanager
    def span(self, name: str, attributes: dict[str, Any]) -> Iterator[Span]:
        span = Span(name, dict(attributes))
        self.spans.append(span)
        try:
            yield span
        except Exception as e:  # noqa: BLE001 - recorded, then re-raised
            span.error = f"{type(e).__name__}: {e}"
            raise


class OtelTracer:
    """OpenTelemetry-backed; AgentCore Observability exports these spans."""

    def __init__(self, tracer: Any) -> None:
        self._tracer = tracer

    @contextmanager
    def span(self, name: str, attributes: dict[str, Any]) -> Iterator[Span]:
        span = Span(name, dict(attributes))
        with self._tracer.start_as_current_span(name) as otel_span:
            for key, value in attributes.items():
                if value is not None:
                    otel_span.set_attribute(key, value)
            try:
                yield span
            except Exception as e:  # noqa: BLE001
                span.error = f"{type(e).__name__}: {e}"
                otel_span.record_exception(e)
                raise


_current: RecordingTracer | OtelTracer | None = None


def tracer() -> RecordingTracer | OtelTracer:
    """The process tracer: OpenTelemetry when present, recording otherwise."""
    global _current  # noqa: PLW0603 - one tracer per process
    if _current is None:
        try:
            from opentelemetry import trace  # type: ignore[import-not-found]

            _current = OtelTracer(trace.get_tracer("memorylane_agents"))
        except ImportError:
            _current = RecordingTracer()
    return _current


def use_tracer(t: RecordingTracer | OtelTracer | None) -> None:
    """Swap the process tracer; tests pass a RecordingTracer, None resets."""
    global _current  # noqa: PLW0603
    _current = t


def workflow_for(detail_type: str) -> str:
    return {
        "trip.detected": "trip_curator",
        "contribution.received": "trip_merge",
        "dedup.proposal": "dedup_steward",
        "cleanup.candidates": "quick_cleanup",
        "nightly": "nightly",
        "nightly.tick": "nightly",
    }.get(detail_type, detail_type)
