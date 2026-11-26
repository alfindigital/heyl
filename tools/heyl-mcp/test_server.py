import unittest
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


class LocalMcpTest(unittest.IsolatedAsyncioTestCase):
    async def test_search_rename_and_reject_stale_title(self):
        server = Path(__file__).with_name("server.py")
        params = StdioServerParameters(command="uv", args=["run", "--no-project", "--script", str(server)])
        async with stdio_client(params) as (read, write), ClientSession(read, write) as session:
            await session.initialize()
            tools = {tool.name: tool for tool in (await session.list_tools()).tools}
            self.assertEqual(set(tools), {"search_logins", "rename_login"})
            self.assertTrue(tools["search_logins"].annotations.readOnlyHint)
            self.assertFalse(tools["rename_login"].annotations.openWorldHint)

            result = await session.call_tool("search_logins", {"query": " HEYL-TEST.EXAMPLE "})
            self.assertFalse(result.isError, result.content)
            entry, = result.structuredContent["entries"]
            self.assertEqual(set(entry), {"login_id", "title", "website_host"})
            self.assertEqual(entry["title"], "HEYL-CLI-TEST-01")

            args = {
                "login_id": entry["login_id"],
                "expected_title": entry["title"],
                "new_title": "HEYL-CLI-TEST-01-RENAMED",
            }
            result = await session.call_tool("rename_login", args)
            self.assertFalse(result.isError, result.content)
            self.assertEqual(result.structuredContent["title"], args["new_title"])

            stale = await session.call_tool("rename_login", {**args, "new_title": "Should not save"})
            self.assertTrue(stale.isError)
            self.assertIn("Search again", stale.content[0].text)
            missing = await session.call_tool("rename_login", {**args, "login_id": "unknown"})
            self.assertTrue(missing.isError)

            result = await session.call_tool("search_logins", {"query": "renamed"})
            self.assertFalse(result.isError, result.content)
            updated, = result.structuredContent["entries"]
            self.assertEqual(updated, {**entry, "title": args["new_title"]})
            result = await session.call_tool("search_logins", {"query": "does-not-exist"})
            self.assertEqual(result.structuredContent["entries"], [])


if __name__ == "__main__":
    unittest.main()
