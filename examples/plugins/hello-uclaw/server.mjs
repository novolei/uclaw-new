#!/usr/bin/env node
/**
 * hello-uclaw — minimal stdio MCP server (JSON-RPC 2.0, line-delimited).
 *
 * Exposes one tool: `hello` — says hello to a named person (or "world").
 *
 * Protocol: one JSON-RPC object per line on stdin; one JSON-RPC object per
 * line on stdout.  Blank lines are ignored.  Notifications (methods starting
 * with "notifications/") receive no reply.
 *
 * Run directly (needs executable bit):  ./server.mjs
 * Or via node:                           node server.mjs
 */

import readline from "readline";

const rl = readline.createInterface({ input: process.stdin, terminal: false });

rl.on("line", (raw) => {
  const line = raw.trim();
  if (!line) return; // ignore blank lines

  let req;
  try {
    req = JSON.parse(line);
  } catch (e) {
    // Malformed JSON — emit a parse-error response with null id.
    reply({ jsonrpc: "2.0", id: null, error: { code: -32700, message: "parse error" } });
    return;
  }

  const { id, method, params } = req;

  // Notifications must not receive a reply.
  if (typeof method === "string" && method.startsWith("notifications/")) {
    return;
  }

  switch (method) {
    case "initialize":
      reply({
        jsonrpc: "2.0",
        id,
        result: {
          protocolVersion: "2024-11-05",
          capabilities: { tools: {} },
          serverInfo: { name: "hello-uclaw", version: "0.1.0" },
        },
      });
      break;

    case "tools/list":
      reply({
        jsonrpc: "2.0",
        id,
        result: {
          tools: [
            {
              name: "hello",
              description: "Say hello to someone.",
              inputSchema: {
                type: "object",
                properties: {
                  name: { type: "string", description: "The name to greet." },
                },
                required: [],
              },
            },
          ],
        },
      });
      break;

    case "tools/call": {
      const toolName = params?.name;
      if (toolName !== "hello") {
        reply({
          jsonrpc: "2.0",
          id,
          error: { code: -32601, message: `tool not found: ${toolName}` },
        });
        break;
      }
      const who = params?.arguments?.name ?? "world";
      reply({
        jsonrpc: "2.0",
        id,
        result: {
          content: [{ type: "text", text: `Hello, ${who}!` }],
        },
      });
      break;
    }

    default:
      reply({
        jsonrpc: "2.0",
        id,
        error: { code: -32601, message: "method not found" },
      });
  }
});

function reply(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}
