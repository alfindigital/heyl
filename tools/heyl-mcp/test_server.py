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
            self.assertEqual(
                set(tools),
                {"create_login", "search_logins", "rename_login", "set_dummy_password", "delete_login"},
            )
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

    async def test_create_rotate_and_delete_one_synthetic_login(self):
        server = Path(__file__).with_name("server.py")
        params = StdioServerParameters(command="uv", args=["run", "--no-project", "--script", str(server)])
        async with stdio_client(params) as (read, write), ClientSession(read, write) as session:
            await session.initialize()
            tools = {tool.name: tool for tool in (await session.list_tools()).tools}
            self.assertIn("create_login", tools, "heyl-local must expose create_login over stdio")
            self.assertFalse(tools["create_login"].annotations.destructiveHint)
            self.assertTrue(tools["delete_login"].annotations.destructiveHint)

            create_args = {
                "title": "CRUD TEST",
                "website_host": " CRUD-TEST.EXAMPLE ",
                "password_fixture": "dummy-a",
            }
            rejected = await session.call_tool("create_login", {**create_args, "website_host": "real.com"})
            self.assertTrue(rejected.isError)
            created = await session.call_tool("create_login", create_args)
            self.assertFalse(created.isError, created.content)
            entry = created.structuredContent
            self.assertEqual(set(entry), {"login_id", "title", "website_host"})
            self.assertEqual(entry["website_host"], "crud-test.example")
            self.assertNotIn("dummy-a", created.content[0].text)

            found = await session.call_tool("search_logins", {"query": "crud-test.example"})
            self.assertEqual(found.structuredContent, {"entries": [entry]})
            renamed = await session.call_tool(
                "rename_login",
                {"login_id": entry["login_id"], "expected_title": entry["title"], "new_title": "Renamed"},
            )
            self.assertEqual(renamed.structuredContent, {**entry, "title": "Renamed"})

            target = {"login_id": entry["login_id"], "expected_title": "Renamed"}
            rotation = {**target, "expected_password_fixture": "dummy-a", "password_fixture": "dummy-b"}
            changed = await session.call_tool("set_dummy_password", rotation)
            self.assertFalse(changed.isError, changed.content)
            self.assertEqual(changed.structuredContent, {"success": True})
            self.assertNotIn("dummy-b", changed.content[0].text)
            stale_password = await session.call_tool("set_dummy_password", rotation)
            self.assertTrue(stale_password.isError)
            self.assertNotIn("dummy-b", stale_password.content[0].text)
            restored = await session.call_tool(
                "set_dummy_password", {**target, "expected_password_fixture": "dummy-b", "password_fixture": "dummy-a"}
            )
            self.assertEqual(restored.structuredContent, {"success": True})

            stale_delete = await session.call_tool("delete_login", {**target, "expected_title": entry["title"]})
            self.assertTrue(stale_delete.isError)
            deleted = await session.call_tool("delete_login", target)
            self.assertEqual(deleted.structuredContent, {"success": True})
            gone = await session.call_tool("search_logins", {"query": "crud-test.example"})
            self.assertEqual(gone.structuredContent, {"entries": []})
            seed = await session.call_tool("search_logins", {"query": "heyl-test.example"})
            self.assertEqual(seed.structuredContent["entries"][0]["title"], "HEYL-CLI-TEST-01")


if __name__ == "__main__":
    unittest.main()
