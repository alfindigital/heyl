import { randomUUID } from "node:crypto";
import { createServer } from "node:http";

const testPath = "/heyl-dummy-fill";
const testPage = `<!doctype html>
<html lang="en">
<meta charset="utf-8">
<title>Heyl dummy password test</title>
<h1>Heyl dummy password test</h1>
<form method="post" autocomplete="off">
  <label for="password">Test password</label>
  <input id="password" type="password" autocomplete="off">
</form>
</html>`;

export async function fillDummyPassword(fixture) {
  if (fixture !== "dummy-a" && fixture !== "dummy-b") {
    throw new Error("The test form accepts built-in dummy fixtures only.");
  }

  let postReceived = false;
  const server = createServer((request, response) => {
    if (request.method === "POST") postReceived = true;
    if (request.method !== "GET" || request.url !== testPath) {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, {
      "Content-Type": "text/html; charset=utf-8",
      "Content-Security-Policy": "default-src 'none'; form-action 'none'; base-uri 'none'",
      "Cache-Control": "no-store",
    }).end(testPage);
  });

  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  try {
    // A separate space keeps the test from changing a tab the user is working in.
    const task = await taskSpace(`Heyl dummy fill ${randomUUID()}`);
    const page = task.page("p1");
    const url = `http://127.0.0.1:${server.address().port}${testPath}`;
    await page.goto(url, { timeout: 10000 });
    await page.waitForSelector("#password", { state: "visible", timeout: 5000 });

    const ready = await page.evaluate(() => {
      const field = document.querySelector("#password");
      field.addEventListener("input", () => { field.dataset.inputReceived = "true"; });
      field.form.addEventListener("submit", (event) => {
        event.preventDefault();
        field.form.dataset.submitted = "true";
      });
      return field.type === "password" && field.value === "";
    });
    if (!ready || (await page.url()) !== url) {
      throw new Error("Ego did not open an empty local password form.");
    }

    await page.fill("#password", fixture);
    const verified = await page.evaluate((expected) => {
      const field = document.querySelector("#password");
      return field?.type === "password" && field.value === expected &&
        field.dataset.inputReceived === "true" && field.form.dataset.submitted !== "true";
    }, fixture);
    if (!verified || (await page.url()) !== url || postReceived) {
      throw new Error("Ego did not verify a masked, unsubmitted password field.");
    }
    await task.finish({ keep: [] });
    console.log("HEYL_DUMMY_FILL_OK");
  } finally {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
}
