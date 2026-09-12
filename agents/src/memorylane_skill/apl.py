"""The APL document for the slideshow: a full-screen Pager of images with a caption strip,
advanced by an AutoPage command. Docs: APL Pager, AutoPage, document settings. Unverified
on a device; the shape follows the published examples."""

from __future__ import annotations

from typing import Any

TOKEN = "memorylane.slideshow"
PAGER_ID = "slides"
SECONDS_PER_SLIDE = 8
# APL settings.idleTimeout is a recommendation the device may bound (documented as such).
IDLE_TIMEOUT_MS = 30 * 60 * 1000


def slideshow_document() -> dict[str, Any]:
    return {
        "type": "APL",
        "version": "2023.3",
        "settings": {"idleTimeout": IDLE_TIMEOUT_MS},
        "mainTemplate": {
            "parameters": ["payload"],
            "items": [
                {
                    "type": "Container",
                    "width": "100vw",
                    "height": "100vh",
                    "items": [
                        {
                            "type": "Pager",
                            "id": PAGER_ID,
                            "width": "100vw",
                            "height": "100vh",
                            "navigation": "wrap",
                            "data": "${payload.slides}",
                            "items": [
                                {
                                    "type": "Container",
                                    "width": "100vw",
                                    "height": "100vh",
                                    "items": [
                                        {
                                            "type": "Image",
                                            "source": "${data.url}",
                                            "width": "100vw",
                                            "height": "100vh",
                                            "scale": "best-fit",
                                            "align": "center",
                                        },
                                        {
                                            "type": "Container",
                                            "position": "absolute",
                                            "bottom": "0",
                                            "width": "100vw",
                                            "padding": "24dp",
                                            "items": [
                                                {
                                                    "type": "Text",
                                                    "text": "${data.title}",
                                                    "color": "#f2f2f5",
                                                    "fontSize": "36dp",
                                                    "fontWeight": "600",
                                                },
                                                {
                                                    "type": "Text",
                                                    "text": "${data.caption}",
                                                    "color": "#d0d0d8",
                                                    "fontSize": "26dp",
                                                },
                                            ],
                                        },
                                    ],
                                }
                            ],
                        }
                    ],
                }
            ],
        },
    }


def render_directive(slides: list[dict[str, str]]) -> dict[str, Any]:
    return {
        "type": "Alexa.Presentation.APL.RenderDocument",
        "token": TOKEN,
        "document": slideshow_document(),
        "datasources": {"payload": {"slides": slides}},
    }


def autopage_directive(count: int, seconds: int = SECONDS_PER_SLIDE) -> dict[str, Any]:
    """Advance through every slide, then stop on the last one."""
    return {
        "type": "Alexa.Presentation.APL.ExecuteCommands",
        "token": TOKEN,
        "commands": [
            {
                "type": "AutoPage",
                "componentId": PAGER_ID,
                "count": max(0, count - 1),
                "duration": seconds * 1000,
                "transitionDuration": 600,
            }
        ],
    }
