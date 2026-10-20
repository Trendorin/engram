#!/usr/bin/env python3
"""README art: the flow diagram, the application icon (from logo.svg)
and the social preview, in English and Russian, light and dark.

    python3 tools/art/build.py

Writes into .github/assets/. Needs fonttools, uharfbuzz and rsvg-convert; engine.py
explains the details.
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from engine import Flow, Node, build  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]

build(
    ROOT, "Engram", ("#6e56f0", "#a397ff"),
    tagline={
        "en": "Memory scanner for Linux, Wine and AI agents",
        "ru": "Сканер памяти для Linux, Wine и ИИ-агентов",
    },
    flows={
        "en": Flow([[Node("Window", "engram-gui"), Node("Command line", "engram"), Node("AI agent", "engram mcp")],
                    [Node("Engine", "engramd", accent=True)],
                    [Node("Your program", "native · Wine · Proton")]]),
        "ru": Flow([[Node("Окно", "engram-gui"), Node("Командная строка", "engram"), Node("ИИ-агент", "engram mcp")],
                    [Node("Движок", "engramd", accent=True)],
                    [Node("Ваша программа", "native · Wine · Proton")]]),
    },
    flow_alt={
        "en": "The window, the CLI and AI agents talk to one engramd session, which reads your program's memory",
        "ru": "Окно, CLI и ИИ-агенты работают с одной сессией engramd, которая читает память программы",
    },
    app_icons=["packaging/io.github.trendorin.Engram.svg"],
)
