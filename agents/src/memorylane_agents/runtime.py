"""AgentCore Runtime entrypoint.

Two payload shapes arrive here: an EventBridge event forwarded by the rule
(``{"detail-type": ..., "detail": {...}}``) and a conversational prompt from the TV or
Alexa+ (``{"household": ..., "prompt": ...}``). Imports of the AgentCore and Strands SDKs
are lazy so the workflow code stays importable without them.
"""

from __future__ import annotations

import os
from dataclasses import asdict, is_dataclass
from typing import Any

from .model import Model, bedrock_model
from .router import handle_event
from .tools import Tools
from .tracing import ATTR_HOUSEHOLD, ATTR_TRIGGER, ATTR_WORKFLOW, tracer, workflow_for
from .workflows import ask_archive, describe_photo


def _plain(value: Any) -> Any:
    return asdict(value) if is_dataclass(value) and not isinstance(value, type) else value


def invoke(payload: dict[str, Any], tools: Tools, model: Model) -> dict[str, Any]:
    """One traced agent run (F-506): the span carries the household and the workflow."""
    if "detail-type" in payload:
        detail = payload.get("detail", {})
        workflow = workflow_for(payload["detail-type"])
        attrs = {
            ATTR_HOUSEHOLD: detail.get("household"),
            ATTR_WORKFLOW: workflow,
            ATTR_TRIGGER: payload["detail-type"],
        }
        with tracer().span(f"agent.{workflow}", attrs):
            result = handle_event(tools, model, payload["detail-type"], detail)
        return {"handled": result is not None, "result": _plain(result)}
    if "question" in payload:
        attrs = {
            ATTR_HOUSEHOLD: payload["household"],
            ATTR_WORKFLOW: "describe_photo",
            ATTR_TRIGGER: "voice",
        }
        with tracer().span("agent.describe_photo", attrs):
            result = describe_photo(
                tools, model, payload["household"], payload["question"], payload.get("tv")
            )
        return {"result": _plain(result)}
    if "prompt" in payload:
        attrs = {
            ATTR_HOUSEHOLD: payload["household"],
            ATTR_WORKFLOW: "ask_archive",
            ATTR_TRIGGER: "prompt",
        }
        with tracer().span("agent.ask_archive", attrs):
            result = ask_archive(
                tools, model, payload["household"], payload["prompt"], payload.get("trip")
            )
        return {"result": _plain(result)}
    return {"error": "unrecognised payload"}


def gateway_tools() -> Tools:
    """MCP client against the AgentCore Gateway. Wired when the Gateway exists."""
    from .gateway import GatewayTools

    return GatewayTools(
        url=os.environ["GATEWAY_URL"],
        token_url=os.environ["GATEWAY_TOKEN_URL"],
        client_id=os.environ["GATEWAY_CLIENT_ID"],
        client_secret=os.environ["GATEWAY_CLIENT_SECRET"],
    )


def main() -> None:  # pragma: no cover - needs the AgentCore SDK and AWS
    from bedrock_agentcore import BedrockAgentCoreApp  # type: ignore[import-not-found]

    app = BedrockAgentCoreApp()
    tools = gateway_tools()
    model = bedrock_model(os.environ.get("MODEL_ID", "us.amazon.nova-pro-v1:0"))

    @app.entrypoint
    def entrypoint(payload: dict[str, Any]) -> dict[str, Any]:
        return invoke(payload, tools, model)

    app.run()


if __name__ == "__main__":  # pragma: no cover
    main()
