# mcp — architecture

## Files

| file | holds |
|---|---|
| `mod.rs` | the `mcp` tool: list and call |
| `bridge.rs` | `McpBridge`, `ServerInfo`, `ToolInfo`, `McpResult`, `McpContent` |
| `setup.rs` | `mcp.json` → the server table, read for the session's cwd; the project's file replaces the global one |
| `sync.rs` | `SyncMcpBridge`: one lazy stdio server per name; a rescan re-reads the table and drops the pool when the table or the directory changed; a stop kills the group of the server a call waits on |
| `stdio.rs` | one stdio server process in a process group of its own: spawn, JSON-RPC, pending map, stderr tail, shutdown |

## Relations

- Depends on `contract::view::Source` for `mcp(session, cwd)`, which builds and rescans the bridge.
- Depends on `platform::proc` and `platform::stop` for the server's process group and the stop
  action that kills it.
- Depends on `workspace`'s cwd: the table is read for where the session stands.
- Consumed by the engine and the registry like every tool.
- Kept in step with `loop/session/source`: the loop supplies the global path and the session's cwd;
  the domain never decides where the session stands.

## Must stay in step

1. `mcp::setup::table` and `loop/session/source`: the loop supplies the global path and the
   session's cwd; the domain never decides where the session stands.
2. `McpBridge` and `SyncMcpBridge`: the seam is the whole surface above the transport; a new
   operation starts there.
3. The stop action and the process group: a stop kills the group of the server a call waits on,
   and the next call starts that server again.