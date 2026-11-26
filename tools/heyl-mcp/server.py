# /// script
# requires-python = ">=3.11"
# dependencies = ["mcp==1.26.0"]
# ///

from typing import Annotated

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


SearchQuery = Annotated[str, StringConstraints(strip_whitespace=True, min_length=1)]
NewTitle = Annotated[str, StringConstraints(strip_whitespace=True, min_length=1)]

server = FastMCP("heyl-local")
dummy = Login(
    login_id="dummy-01",
    title="HEYL-CLI-TEST-01",
    website_host="heyl-test.example",
)


@server.tool(annotations=ToolAnnotations(readOnlyHint=True, openWorldHint=False))
async def search_logins(query: SearchQuery) -> SearchResult:
    """Search synthetic login titles and website hosts, ignoring case."""
    term = query.casefold()
    matched = term in dummy.title.casefold() or term in dummy.website_host.casefold()
    return SearchResult(entries=[dummy.model_copy()] if matched else [])


@server.tool(
    annotations=ToolAnnotations(
        readOnlyHint=False, destructiveHint=True, idempotentHint=False, openWorldHint=False
    )
)
async def rename_login(login_id: str, expected_title: str, new_title: NewTitle) -> Login:
    """Rename a synthetic login using the ID and exact title from search_logins."""
    if login_id != dummy.login_id:
        raise ToolError("Unknown login ID. Search again and use the returned ID.")
    if expected_title != dummy.title:
        raise ToolError("Title changed. Search again before retrying.")
    # The comparison and update run without a suspension point.
    dummy.title = new_title
    return dummy.model_copy()


if __name__ == "__main__":
    server.run(transport="stdio")
