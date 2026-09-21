"""The language model behind narratives and intent parsing, behind a one-method protocol."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Protocol


class Model(Protocol):
    model_id: str

    def complete(self, system: str, prompt: str) -> str: ...


@dataclass
class ScriptedModel:
    """Returns canned answers in order. For tests."""

    answers: list[str]
    model_id: str = "scripted"
    prompts: list[tuple[str, str]] = field(default_factory=list)

    def complete(self, system: str, prompt: str) -> str:
        self.prompts.append((system, prompt))
        return self.answers.pop(0)


def bedrock_model(model_id: str = "us.amazon.nova-pro-v1:0") -> Model:
    """A Bedrock model (Amazon Nova Pro by default) via the Strands model provider.

    Lazy import: tests need no SDK.
    """
    from strands.models import BedrockModel  # type: ignore[import-not-found]

    class _Adapter:
        def __init__(self) -> None:
            self.model_id = model_id
            self._model = BedrockModel(model_id=model_id, temperature=0.4)

        def complete(self, system: str, prompt: str) -> str:
            from strands import Agent  # type: ignore[import-not-found]

            agent = Agent(model=self._model, system_prompt=system, tools=[])
            return str(agent(prompt))

    return _Adapter()
