// Drive the release binary over MCP/stdio and assert the whole exchange.
// Every byte on stdout must be a protocol frame.
import { spawn } from "node:child_process";

const bin = process.env.OMMA_BIN ?? "target/release/omma";
const timeoutMs = 60_000;

const requests = [
  {
    jsonrpc: "2.0",
    id: 1,
    method: "initialize",
    params: {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: "omma-ci", version: "0" },
    },
  },
  { jsonrpc: "2.0", method: "notifications/initialized" },
  { jsonrpc: "2.0", id: 2, method: "tools/list" },
  {
    jsonrpc: "2.0",
    id: 3,
    method: "tools/call",
    params: { name: "omma_capabilities", arguments: {} },
  },
  {
    jsonrpc: "2.0",
    id: 4,
    method: "tools/call",
    params: {
      name: "analyze_image",
      arguments: { image_path: "tests/fixtures/text.png" },
    },
  },
  {
    jsonrpc: "2.0",
    id: 5,
    method: "tools/call",
    params: { name: "analyze_image", arguments: { image_path: "/nope/nope.png" } },
  },
];

const fail = (message) => {
  console.error(`mcp: ${message}`);
  child.kill();
  process.exit(1);
};

const child = spawn(bin, ["mcp", "serve"], { stdio: ["pipe", "pipe", "pipe"] });
child.on("error", (err) => {
  console.error(`mcp: cannot start server: ${err.message}`);
  process.exit(1);
});

let stderr = "";
child.stderr.on("data", (chunk) => (stderr += chunk));

const messages = [];
let buffer = "";
let finished = false;

const finish = () => {
  if (finished) return;
  finished = true;
  const byId = (id) => messages.find((m) => m.id === id);

  const init = byId(1);
  if (!init?.result) fail(`handshake failed: ${JSON.stringify(init)}`);
  if (init.result.serverInfo?.name !== "omma") fail("serverInfo.name");
  if (!init.result.capabilities?.tools) fail("tools capability missing");

  const tools = byId(2);
  if (!tools?.result?.tools) fail("tools/list returned nothing");
  const names = tools.result.tools.map((t) => t.name);
  for (const want of ["analyze_image", "omma_capabilities"]) {
    if (!names.includes(want)) fail(`tool ${want} missing (got ${names.join(",")})`);
  }
  const analyze = tools.result.tools.find((t) => t.name === "analyze_image");
  if (analyze.inputSchema?.required?.[0] !== "image_path") fail("input schema");
  if (analyze.annotations?.readOnlyHint !== true) fail("readOnlyHint");

  const caps = byId(3);
  if (caps?.result?.isError === true) fail("omma_capabilities returned an error");
  const capsJson = JSON.parse(caps.result.content[0].text);
  if (capsJson.schema_version !== "1.0") fail("capabilities schema_version");
  if (capsJson.ocr?.available !== true) fail("capabilities should report OCR available in CI");
  if (caps.result.structuredContent?.tool?.name !== "omma") fail("structuredContent");

  const report = byId(4);
  if (report?.result?.isError === true) fail("analyze_image returned an error");
  const value = JSON.parse(report.result.content[0].text);
  if (value.image?.width !== 600) fail("analyzed image width");
  if (value.ocr?.untrusted !== true) fail("ocr untrusted marker");
  if (!/omma reads this line/i.test(value.ocr.text)) fail(`ocr text: ${JSON.stringify(value.ocr.text)}`);
  if (report.result.structuredContent?.grid?.width !== 64) fail("structuredContent grid");

  const missing = byId(5);
  if (missing?.result?.isError !== true) fail("a missing file must be a tool error");

  child.kill();
  console.log(
    `mcp ok: ${messages.length} frames, tools=${names.join(",")}, ` +
      `ocr=${value.ocr.confidence}, regions=${value.layout.regions.length}`,
  );
  process.exit(0);
};

child.stdout.on("data", (chunk) => {
  buffer += chunk;
  let index;
  while ((index = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, index).trim();
    buffer = buffer.slice(index + 1);
    if (!line) continue;
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      fail(`non-JSON on stdout: ${JSON.stringify(line.slice(0, 120))}`);
    }
    if (message.jsonrpc !== "2.0") fail(`non-protocol stdout: ${line.slice(0, 120)}`);
    messages.push(message);
    // rmcp dispatches requests concurrently, so a slow analyze may answer after
    // a fast error. Wait until every expected id has arrived.
    if ([1, 2, 3, 4, 5].every((id) => messages.some((m) => m.id === id))) finish();
  }
});

const timer = setTimeout(() => fail("timed out waiting for the tool responses"), timeoutMs);
timer.unref?.();

for (const request of requests) {
  child.stdin.write(JSON.stringify(request) + "\n");
}
child.stdin.end();

process.on("exit", () => {
  if (stderr.trim()) console.log(`server stderr:\n${stderr.trim()}`);
});
