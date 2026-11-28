# /// script
# requires-python = ">=3.11"
# dependencies = ["mcp==1.26.0"]
# ///

import asyncio
import json
import os
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Annotated, Literal
from uuid import uuid4

from mcp.server.fastmcp import FastMCP
from mcp.server.fastmcp.exceptions import ToolError
from mcp.types import ToolAnnotations
from pydantic import BaseModel, StringConstraints


class Login(BaseModel):
    login_id: str
    title: str
    website_host: str


class SearchResult(BaseModel):
    entries: list[Login]


class OperationResult(BaseModel):
    success: Literal[True] = True


SearchQuery = Annotated[str, StringConstraints(strip_whitespace=True, min_length=1)]
NewTitle = Annotated[str, StringConstraints(strip_whitespace=True, min_length=1)]
ExampleHost = Annotated[
    str,
    StringConstraints(
        strip_whitespace=True,
        to_lower=True,
        max_length=253,
        pattern=r"^(?:[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)+[eE][xX][aA][mM][pP][lL][eE]$",
    ),
]
PasswordFixture = Literal["dummy-a", "dummy-b"]


@dataclass
class LoginRecord:
    metadata: Login
    password_fixture: PasswordFixture | None = None


server = FastMCP("heyl-local")
records = {
    "dummy-01": LoginRecord(
        metadata=Login(
            login_id="dummy-01",
            title="HEYL-CLI-TEST-01",
            website_host="heyl-test.example",
        ),
    ),
}


def get_current_record(login_id: str, expected_title: str) -> LoginRecord:
    record = records.get(login_id)
    if record is None:
        raise ToolError("Unknown login ID. Search again and use the returned ID.")
    if expected_title != record.metadata.title:
        raise ToolError("Title changed. Search again before retrying.")
    return record


async def fill_in_ego(fixture: PasswordFixture) -> None:
    executable = shutil.which("ego-browser.exe" if os.name == "nt" else "ego-browser")
    if executable is None and os.name == "nt":
        # The standard Windows install exposes a .cmd shim on PATH.
        installed = Path.home() / ".local/share/ego/active_version_dir/ego-browser.exe"
        if installed.is_file():
            executable = str(installed)
    if executable is None:
        raise ToolError("Ego Browser CLI is required for the dummy fill test.")

    module_url = Path(__file__).resolve().with_name("ego_fill.mjs").as_uri()
    script = (
        f"const {{ fillDummyPassword }} = await import({json.dumps(module_url)});\n"
        f"await fillDummyPassword({json.dumps(fixture)});\n"
    )
    try:
        completed = await asyncio.to_thread(
            subprocess.run,
            [executable, "nodejs"],
            input=script,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            encoding="utf-8",
            errors="replace",
            timeout=45,
        )
    except subprocess.TimeoutExpired:
        raise ToolError("Ego Browser timed out during the dummy fill test.") from None
    except OSError:
        raise ToolError("Could not start the Ego Browser CLI.") from None
    if completed.returncode != 0 or "HEYL_DUMMY_FILL_OK" not in completed.stdout.splitlines():
        raise ToolError("Ego did not verify the dummy fill. Check that Ego Desktop is running.")


@server.tool(
    annotations=ToolAnnotations(
        readOnlyHint=False, destructiveHint=False, idempotentHint=False, openWorldHint=False
    )
)
async def create_login(
    title: NewTitle,
    website_host: ExampleHost,
    password_fixture: PasswordFixture | None = None,
) -> Login:
    """Create an in-memory test login at a .example host, optionally with a built-in password fixture."""
    login_id = f"dummy-{uuid4().hex}"
    metadata = Login(login_id=login_id, title=title, website_host=website_host)
    records[login_id] = LoginRecord(metadata=metadata, password_fixture=password_fixture)
    return metadata.model_copy()


@server.tool(annotations=ToolAnnotations(readOnlyHint=True, openWorldHint=False))
async def search_logins(query: SearchQuery) -> SearchResult:
    """Search synthetic login titles and website hosts, ignoring case."""
    term = query.casefold()
    entries = (record.metadata for record in records.values())
    return SearchResult(
        entries=[
            entry.model_copy()
            for entry in entries
            if term in entry.title.casefold() or term in entry.website_host.casefold()
        ]
    )


@server.tool(
    annotations=ToolAnnotations(
        readOnlyHint=False, destructiveHint=True, idempotentHint=False, openWorldHint=False
    )
)
async def rename_login(login_id: str, expected_title: str, new_title: NewTitle) -> Login:
    """Rename a synthetic login using the ID and exact title from search_logins."""
    record = get_current_record(login_id, expected_title)
    record.metadata.title = new_title
    return record.metadata.model_copy()


@server.tool(
    annotations=ToolAnnotations(
        readOnlyHint=False, destructiveHint=True, idempotentHint=False, openWorldHint=False
    )
)
async def set_dummy_password(
    login_id: str,
    expected_title: str,
    expected_password_fixture: PasswordFixture | None,
    password_fixture: PasswordFixture,
) -> OperationResult:
    """Set or replace a built-in password fixture. Pass null as the expected fixture if none is set."""
    record = get_current_record(login_id, expected_title)
    if record.password_fixture != expected_password_fixture:
        raise ToolError("Password fixture changed. Use the fixture last set successfully before retrying.")
    record.password_fixture = password_fixture
    return OperationResult()


@server.tool(
    annotations=ToolAnnotations(
        readOnlyHint=False, destructiveHint=False, idempotentHint=False, openWorldHint=False
    )
)
async def fill_dummy_password(login_id: str, expected_title: str) -> OperationResult:
    """Fill and verify a synthetic login's password on a local test form in Ego Browser."""
    record = get_current_record(login_id, expected_title)
    if record.password_fixture is None:
        raise ToolError("No dummy password is set. Use set_dummy_password first.")
    await fill_in_ego(record.password_fixture)
    return OperationResult()


@server.tool(
    annotations=ToolAnnotations(
        readOnlyHint=False, destructiveHint=True, idempotentHint=False, openWorldHint=False
    )
)
async def delete_login(login_id: str, expected_title: str) -> OperationResult:
    """Delete a synthetic login using its ID and exact current title from search_logins."""
    get_current_record(login_id, expected_title)
    del records[login_id]
    return OperationResult()


if __name__ == "__main__":
    server.run(transport="stdio")
