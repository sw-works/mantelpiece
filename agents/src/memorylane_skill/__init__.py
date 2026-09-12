"""MemoryLane custom Alexa skill: trips as a slideshow on an Echo Show.

Stdlib only, so the Lambda ships the source directory as is. `handler.handle` takes the
request envelope and a `Backend`; `lambda_handler` wires the HTTP backend from the
environment.
"""

from .handler import handle, lambda_handler

__all__ = ["handle", "lambda_handler"]
