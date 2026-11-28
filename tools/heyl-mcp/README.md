## Local MCP prototype

This stdio server keeps synthetic logins in memory. Each server process starts
with `HEYL-CLI-TEST-01` at `heyl-test.example`. Restarting the process resets
all entries and password fixtures. The prototype needs Python 3.11+ and `uv`,
and runs without a Heyl binary, heylogin credentials, or a browser session.

Register it with Codex from PowerShell in the root of the Heyl checkout.

```powershell
$server = (Resolve-Path -LiteralPath '.\tools\heyl-mcp\server.py').Path
codex mcp add heyl-local -- uv run --no-project --script $server
```

Try the full flow in one new Codex session.

1. Use `create_login` with title `CRUD TEST`, host `crud-test.example`, and
   optional `password_fixture` set to `dummy-a`. Create accepts .example
   hostnames only and generates a fresh ID for each call.
2. Use `search_logins` to find the entry, then `rename_login` with its ID and
   exact current title to rename it to `CRUD RENAMED`.
3. Use `set_dummy_password` with the new title, `expected_password_fixture`
   set to `dummy-a`, and `password_fixture` set to `dummy-b`. These are the
   two built-in fixture values. Pass `null` as the expected fixture when no
   fixture is set. The tool returns only `{"success": true}` on success.
4. Use `delete_login` with the same ID and current title, then search again
   for `crud-test.example` to verify that the result is empty.

Search and create return metadata without the password fixture. Updates and
deletion reject a stale title. Changing the password fixture also rejects a
stale expected fixture.

### Try dummy autofill in Ego Browser

Open Ego Desktop with its CLI installed. Start a new Codex session to load
`fill_dummy_password`, then try:

> Create a dummy login for `ego-fill-test.example` with the `dummy-b` fixture,
> find it with `search_logins`, and run `fill_dummy_password` on the result.

Pass the entry's `login_id` and current title as `expected_title`. The tool
opens a temporary form on `127.0.0.1`, fills the masked field in Ego Browser,
and closes the test tab after verifying the fill without submitting the form.
The result is `{"success": true}`. An entry without a fixture needs
`set_dummy_password` first. The target is always the tool's local form.

Run the stdio integration test from this directory.

```powershell
uv run --no-project --with 'mcp==1.26.0' python test_server.py
```
