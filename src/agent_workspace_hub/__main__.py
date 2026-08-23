"""Command-line entry point for Agent Workspace Hub."""
from __future__ import annotations

import argparse
import asyncio

from .config.settings import get_settings
from .mcp.server import MCPServer
from .tui.app import AgentWorkspaceHubApp


def build_parser() -> argparse.ArgumentParser:
    """Build the Agent Workspace Hub CLI parser."""
    parser = argparse.ArgumentParser(
        prog="awh",
        description="Run the Agent Workspace Hub TUI or MCP server.",
    )
    subparsers = parser.add_subparsers(dest="command")

    subparsers.add_parser("tui", help="Launch the Textual user interface (default).")

    server_parser = subparsers.add_parser("server", help="Run only the MCP SSE server.")
    server_parser.add_argument("--host", help="Host to bind the MCP server to.")
    server_parser.add_argument("--port", type=int, help="Port to bind the MCP server to.")

    return parser


async def run_server(host: str | None = None, port: int | None = None) -> None:
    """Run the MCP server until interrupted."""
    settings = get_settings()
    server = MCPServer(
        host=host or settings.server_host,
        port=port or settings.server_port,
    )
    await server.start()
    try:
        while True:
            await asyncio.sleep(3600)
    except (KeyboardInterrupt, asyncio.CancelledError):
        await server.stop()


def run_tui() -> None:
    """Launch the Textual TUI."""
    AgentWorkspaceHubApp().run()


def main(argv: list[str] | None = None) -> None:
    """Run the Agent Workspace Hub command-line interface."""
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.command == "server":
        asyncio.run(run_server(host=args.host, port=args.port))
        return

    run_tui()


if __name__ == "__main__":
    main()
