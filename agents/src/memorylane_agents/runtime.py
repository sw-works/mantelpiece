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
from .workflows import ask_archive


def _plain(value: Any) -> Any:
    return asdict(value) if is_dataclass(value) and not isinstance(value, type) else value


def invoke(payload: dict[str, Any], tools: Tools, model: Model) -> dict[str, Any]:
    if "detail-type" in payload:
        result = handle_event(tools, model, payload["detail-type"], payload.get("detail", {}))
        return {"handled": result is not None, "result": _plain(result)}
    if "prompt" in payload:
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
        token_provider=os.environ.get("GATEWAY_TOKEN_PROVIDER", "cognito"),
    )


def main() -> None:  # pragma: no cover - needs the AgentCore SDK and AWS
    from bedrock_agentcore import BedrockAgentCoreApp  # type: ignore[import-not-found]

    app = BedrockAgentCoreApp()
    tools = gateway_tools()
    model = bedrock_model(os.environ.get("MODEL_ID", "us.anthropic.claude-sonnet-4-20250514-v1:0"))

    @app.entrypoint
    def entrypoint(payload: dict[str, Any]) -> dict[str, Any]:
        return invoke(payload, tools, model)

    app.run()


if __name__ == "__main__":  # pragma: no cover
    main()
