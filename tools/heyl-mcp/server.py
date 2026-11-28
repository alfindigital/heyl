# /// script
# requires-python = ">=3.11"
# dependencies = ["mcp==1.26.0"]
# ///

from dataclasses import dataclass
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
