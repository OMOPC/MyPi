# mcp

One static tool with a fixed schema. Which servers exist and which tools each exposes is decided at
runtime, so a new server needs no Rust change — only a line in `mcp.json`.

## 1. Design / Schema

### One call

| field | value |
|---|---|
| `type` | `list`, `call` |
| `calls` | `call`: one item per call, with `server`, `tool`, `args` (object) |
| `server` | `list`: the server to list |

`list` is a query operation and refuses an array.

### Servers

The bridge belongs to a session: the tool asks `Source::mcp(session, cwd)`, which builds one on the
first call and follows the session's cwd from then on (a move rescans the table and drops the old
pool). A session that never calls `mcp` never starts a server.

`mcp/setup.rs` reads the table for where the session stands: `<cwd>/.mypi/mcp.json` when it exists,
else `<global root>/mcp.json`.

**The project's file wins whole.** If it exists the global one is not read at all; the two are
never merged. Neither existing means an empty table, and an empty table still mounts the bridge.
The table is read where the session stood at its first `mcp` call; the next call after a `cd` reads
it again, and a changed table or directory drops the process pool. An unchanged one is a no-op.

```json
{
  "servers": {
    "local": {
      "command": "uv",
      "args": ["run", "python", "service.py"],
      "env": {},
      "timeout_secs": 120
    }
  }
}
```

`McpBridge` is the seam: `servers()`, `tools(server)`, `call(server, tool, args, stop)`,
`rescan(cwd)`, `shutdown()`. Nothing above it knows JSON-RPC. `SyncMcpBridge` is the built-in
implementation: one child process per name, started lazily and kept until a rescan; a server spawns
in the session's working directory, so a `cwd` key in the file is not used. stdout is one JSON-RPC
object per line, a reader thread dispatches by id, stderr is drained into a 20-line tail that only
surfaces in errors; every call carries a deadline; a dead process fails every waiting call and is
started again on the next one. Every server leads a process group of its own, and a stop kills that
group: the wait ends on the same tick, and the failure a dead server gives is the call's answer. It
uses threads and channels only — no async runtime enters this tree.

### Answers

An answer is text content, concatenated in order. `isError` makes the call failed. Images, audio
and structured content are reported as unsupported. MCP's own schema is not passed to the model and
not checked here; a bad argument is the server's error to report.

`list` answers JSON, and the same value rides along as the memo. The history summary names the
first server and tool of a `call`, or the listed server.

## 2. Design intent

- **One stdio server per name.** Servers start lazily and stay up for the process; the tool only
  knows names, so a new server is a line of JSON.
- **The project's table wins whole.** Two files are never merged, so a project can replace the
  global set without inheriting a server it did not name.
- **No JSON-RPC above the seam.** The tool layer knows `McpBridge`; the transport is one
  implementation behind it.

## 3. Unresolved

1. MCP carries text content only; a server-initiated request is answered `-32601`.

## 4. Config fields

None. The server table lives in `mcp.json`, not in the tool layer's config.