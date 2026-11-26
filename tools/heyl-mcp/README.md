## Local MCP prototype

This stdio server runs a synthetic login in memory. Each server process starts
with `HEYL-CLI-TEST-01` at `heyl-test.example`. Restarting the process resets
the title. The prototype needs Python 3.11+ and `uv`, and runs without a Heyl
binary, heylogin credentials, or a browser session.

Register it with Codex from PowerShell in the root of the Heyl checkout.

```powershell
$server = (Resolve-Path -LiteralPath '.\tools\heyl-mcp\server.py').Path
codex mcp add heyl-local -- uv run --no-project --script $server
```

In a new Codex session, ask it to search for `heyl-test.example`, rename that
entry to `HEYL-CLI-TEST-01-RENAMED`, then search again. The rename tool takes
the login ID and the current title from the search result. A stale expected
title returns an error and leaves the entry alone.

Run the stdio integration test from this directory.

```powershell
uv run --no-project --with 'mcp==1.26.0' python test_server.py
```
