"""MCP server exposing the headless A320 systems core to LLM agents.

One of the two frontends onto the same control/observe API (the other is the
human REPL, `a320_cli`). The nine tools are a 1:1 mapping onto `a320_sim.Sim`;
there is no simulation logic in this package.
"""

from a320_mcp.server import main, mcp

__all__ = ["main", "mcp"]
__version__ = "0.1.0"
