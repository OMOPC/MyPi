# tools — architecture

What is here, what each file holds, who talks to whom. Current state only.

## Files

| file | holds |
|---|---|
| `mod.rs` | module declarations and re-exports |
| `contract/mod.rs` | declares the six contract groups |
| `contract/call.rs` | `Batch`, `Cwd` |
| `contract/output.rs` | `ToolResult`, `ToolOutput`, `Effect`, `MemoOp`, `AliasOp` |
| `contract/env.rs` | `Env`, `Defaults`, `Limits`, `BashEnv`, `Paths`, `Seen`, `Stamp`, the model table (`Models`, `ModelFacts`) and the child roster derived from it (`roster`) |
| `contract/halt.rs` | the one sentence a stopped turn leaves: `INTERRUPTED`, and the answer a tool gives when nothing was written |
| `contract/view.rs` | the `Source`, `Export`, `Child`, `Draft`, `MemoRead` ports, the history shapes, and `MemoScope`, `MemoReadOp`, `MemoVersion`, `MemoView` |
| `contract/job.rs` | the job vocabulary a tool shares with the loop: `JobSpec`, `Schedule`, `JobRunner`, `JobCtx`, `JobOutcome`, `Receipt`, `Row`, `Status`, `Exit`, `OutputSink`, and the `JobHost` port |
| `engine.rs` | `Engine`: `Batch::run` — env, gates, decode, run, stamp, measure (every tool but `read`), events; a call whose arguments arrived answers, and a stop landing mid-call becomes the result's reason |
| `spec.rs` | `Kind`, `Param`, `schema_of`, `Args`, `Item`, `Tool`, `Run`, `Summary`, `ToolProgress` |
| `summary.rs` | one recorded call's arguments, and the clip every tool's history summary reuses |
| `registry.rs` | `TOOLS`, `spec`, `roster` |
| `workspace.rs` | `Workspace`, the licensed zones, the per-path write lock, `write_atomic`, `canonicalize_lenient` |
| `fs/mod.rs` | declares the fs family |
| `fs/read/` | the `read` tool, directory listings and the module graph; see `fs/read/ARCHITECTURE.md` |
| `fs/grep/` | the `grep` tool; see `fs/grep/README.md` |
| `fs/write/{mod,tool}.rs` | `write`: the content through the formatter, the write whatever the formatter said, then the compiler's report unless the text does not parse |
| `fs/edit/{mod,tool}.rs` | `edit`: one unique run of text, with `exact_text` and the one-line receipt |
| `fs/splice/{mod,tool}.rs` | `splice`: whole lines by number, many ranges at once; the copy and cut macros filled from the snapshot before anything is applied; the batch's calls on one file merged into one write; every edit written except a cross-call overlap; the compiler's faults split onto the lines they hit; `mass_text` and `parse_range` for `final` |
| `fs/splice/edit.rs` | one change: its range, its block, the macro ranges it fills and the changes it cuts, the overlap rule, the reverse-order splice, and where its lines land without a formatter |
| `fs/gate.rs` | the process gate: one child run to its end under a deadline, its whole group killed by a stop or a deadline, its two streams read on threads of their own, and where a toolchain program is found |
| `fs/skeleton.rs` | the report's skeleton over a line table: the innermost piece holding a line, the seam drawn around a range |
| `fs/fault.rs` | the batch's audit: one check per unit written into, by the language that owns it, every error and warning landing once — under the edit whose lines it hit, under the write that replaced the file whole, or in the unit's list on the last answer that can take it |
| `fs/splice/fmt.rs` | the language's formatter through the gate: the merged text with a marker comment around every block, the markers' lines as each change's landing, the text without them, and the per-block trials that only name what the parser blames |
| `fs/splice/receipt.rs` | the receipt, shaped like `grep`'s hits: the verdict, then `path:line`, the item, the lines taken and the block in place, the reasons under them, and the segment memo a front end draws |
| `fs/change.rs` | `context` (the piece around a changed line, its breadcrumb and excerpt), `receipt`, `line_of`, `lines_of` |
| `fs/lang/mod.rs` | the registry: the `Language` every sorter answers with, the list of languages, the suffix lookup, and the two questions forwarded to the language that wrote a table |
| `fs/lang/table.rs` | one file's line table: each line's stamp, the shape it reads by, the line it names, its other end, the language that wrote it; the pieces holding a line, the shallowest pieces of a range, roots and children |
| `fs/lang/verdict.rs` | what a formatter and a checker answer: the formatted text, the text that did not parse, one unit's check, one diagnostic |
| `fs/lang/guages/mod.rs` | declares the languages |
| `fs/lang/guages/rust/mod.rs` | declares the Rust sorter and its scan |
| `fs/lang/guages/rust/table.rs` | the Rust sorter: its suffix, head words, key-line passes, head templates, and what kind of piece a line opens |
| `fs/lang/guages/rust/relations.rs` | Rust's relation index: `A::b` as a module or type member, a lone name's declarations and impl blocks, and how it reads `::` |
| `fs/lang/guages/rust/scan.rs` | Rust's own scan: a cursor and a line buffer Rust's delimiters drive, and the brace pairs counted on the result |
| `fs/lang/guages/markdown.rs` | the Markdown sorter: headings, fences, lists, quotes, HTML blocks, front matter, and how far every heading's section reaches |
| `shell/mod.rs` | declares the shell family |
| `shell/bash/mod.rs` | declares the command tool, the admission gate, the segment scanner, the guard and the runner |
| `shell/bash/tool.rs` | the `bash` tool |
| `shell/bash/admit.rs` | `admit`: aliases → `#ref` → guard, and the line it hands the runner |
| `shell/bash/segment.rs` | the separator set and the segment heads; the guard and the alias expansion share both |
| `shell/bash/guard.rs` | the command classifier: `normalize`, `classify`, `Verdict`, `RuleHit` |
| `shell/bash/shell.rs` | one command line: `run`, `run_to`, `OutputSink`, `Exit`, `StopFlag`; registers the stop action, spawns and signals through `platform::proc` |
| `shell/cd/{mod,tool}.rs` | `cd` |
| `shell/alias/{mod,tool}.rs` | `alias` add/list/remove; the config layer is read-only here |
| `kairos/mod.rs` | declares the kairos modules |
| `kairos/tool.rs` | the `kairos` tool manual and parameters |
| `kairos/service.rs` | `kairos` operation dispatch and rendering |
| `kairos/command.rs` | a line admitted at call time as a `JobRunner`: only `run_to` is left |
| `kairos/text.rs` | the text `JobRunner`: nothing to run, so only the clock makes it due |
| `kairos/duration.rs` | relative duration parse (`10min`, `1h30m`) |
| `mcp/mod.rs` | the `mcp` tool: list and call |
| `mcp/bridge.rs` | `McpBridge`, `ServerInfo`, `ToolInfo`, `McpResult`, `McpContent` |
| `mcp/setup.rs` | `mcp.json` → the server table, read for the session's cwd; the project's file replaces the global one |
| `mcp/sync.rs` | `SyncMcpBridge`: one lazy stdio server per name; a rescan re-reads the table and drops the pool when the table or the directory changed; a stop kills the group of the server a call waits on |
| `mcp/stdio.rs` | one stdio server process in a process group of its own: spawn, JSON-RPC, pending map, stderr tail, shutdown |
| `memo/mod.rs` | declares the memo family |
| `memo/context/{mod,tool}.rs` | `context`: `query` through `Source::history`, `show` through `Source::block`, long listings to `Export::dir` |
| `memo/memo/{mod,tool}.rs` | the `memo` tool: writes as effects, reads through `MemoRead` |
| `memo/todo/{mod,tool}.rs` | the `todo` tool: reads the anchor, replays, applies, writes the memo |
| `memo/todo/replay.rs` | the pure replay: a call sequence → the list, anchored on the newest reset |
| `memo/todo/ops.rs` | the three todo operations (`set`, `tick`, `view`) and their batch apply |
| `memo/todo/state.rs` | `TodoItem`, `counts` |
| `subagent/mod.rs` | declarations and re-exports (`TASK`, `FINAL`), and `project_dir` |
| `subagent/task.rs` | the `task` tool: the call's project folder, template, variables, answer name, the child's model, the `Child` effect |
| `subagent/final.rs` | the `final` tool: the child's draft, its edits, the one submit |
| `subagent/template.rs` | template lookup (project, then global, then the named path), front matter, `{{var}}` rendering, roster subset checks, the stem |
| `subagent/answer.rs` | the answer file: the first free name inside the project's `answers` folder, taken against the names one call already handed out, and the one stamped write |
| `web/mod.rs` | the `web` tool entry: arguments to `WebArgs`, one call to the domain |
| `search/mod.rs` | the `search` tool entry: the same `WebArgs`, one query against one engine |

## What it talks to

- **`storage`**: not imported here at all; a tool reaches the database only through `Source` and
  `Export`, both answered by the loop.
- **`config`**: not imported by any tool. The loop reads the keys and hands values in, except the
  browser keys, which the web domain reads itself.
- **`platform`**: `time::now_stamp` (the effect stamp) and the local-time parts, `log::warn`,
  `env::temp_dir`, `shell::Shell` (the session's shell: program, one-shot argument shape,
  stored-text path form), `proc` (process group spawn, group signal, tree teardown), `stop::Stop`
  (the session's stop signal and the actions a call registers on it).
- **`request`**: `ToolCall`, `ToolDef`, `Message` for definitions and the context window.
- **`web`**: `WebArgs`, `run` from the `web` tool; `shutdown` from `main`.
- **Who calls it**: `loop/turn` (one batch at a time, `Batch::run`), `loop/turn/cycle` (`roster`
  for the manuals, and the child list correction), `loop/session/book` (builds `Engine` from
  config), `loop/session/source` (builds a session's job service and MCP bridge on first use).
- **Knows nothing about**: the api doors, the database connection, the front end, rounds, streams.

## Inside

```text
Engine::run
  → registry::spec              and env.allows
  → Args::fill
  → Tool::run
      fs/*            ── file tools and one line table they all read
      fs/grep         ── scan (the walk) ── lang (the table a sorter answers)
      kairos          ── Source::jobs (a JobHost) ── loop/job
      task            ── template, answer name ── Effect::Child
      final           ── Source::child (a Child) ── draft, answer file
      mcp             ── mcp::sync ── mcp::stdio
      web, search     ── crate::web
      memo/*          ── memo, todo, context
  → effect stamping → spill → ToolOutput
```

- `Env` is built once when a session is booked, from `Defaults`; the engine runs it as it stands.
  Its child roster is derived at the same moment: every model flagged `subagent` plus the
  session's own, and a model switch rebuilds it.
- A `JobHost` and a sub-session's `Child` reach a tool through `Source`; the loop attaches both when
  it books a session and takes both off when it drops one. A job is only ever created by the loop.
- The guard is used in two places — the `bash` tool and `alias add` — and judges the expanded
  command in both.
- The MCP bridge is built by `Source::mcp(session, cwd)` on the session's first `mcp` call, from the
  table for its cwd; a later move rescans it. A broken table fails that call, not the turn. An empty
  table still mounts the bridge.
- A child's tool list is corrected once, where the child is spawned: `task` out, `final` in. A main
  session never resolves `final`: `api::post::init` drops it from the list and its manuals.

## Must stay in step

1. `contract::env::Env` and `engine`: every field is filled in one place; a new port is a field here
   and a trait in `contract::view`.
2. `spec::Param` order and each tool's index constants: `Args` reads positionally at the top
   level, and `Args::objects` reads an array item's fields by name against the item table under
   `Kind::Objects`.
3. `schema_of` and `Args::fill`: one `Param` table feeds both; `intent` is added to the flat
   schema in one place and the same fields are read by position in the other.
4. `ToolOutput.effects` and `contract::output::Effect`: the front door stamps `ts`, so a tool must
   never fill it.
5. `bash` and `kairos`: every command goes through `admit`; the guard runs once, on the final text.
6. `licensed_zones` and `guard::escapes_zone`: two independent zone laws; changing one does not
   change the other.
7. `subagent::template` and the parent roster: a template may only name tools the parent has.
8. `contract::job::JobHost` and `loop/job`: the port is the whole surface a tool sees; a new job
   field starts there.
9. `Args::fill`'s field check and every tool's parameter table: a name the table does not hold is
   refused, so a new argument is added to the table or it cannot be passed.
10. `mcp::setup::table` and `loop/session/source`: the loop supplies the global path and the session's
    cwd; the domain never decides where the session stands.
11. The web domain's config keys and `src/web/README.md`: it reads them itself, so a rename here is
    a rename there.
12. `platform::stop::Stop` and its three holders — the session handle, each job's `cancel`, and
    `Env.stop`: one type, one flag, no second way to say stopped.
13. `Pen::drop_calls` and `ask.results`: a call without a result has its request block dropped
    before the commit; a result without a call is refused.
14. `ToolOutput.fail` and `engine`: any reason fails the call, and a call cut short states it in the
    one item it answered; the front door adds one for the stop it observed, and the tool reports its
    own deadline.
15. `fs/fault.rs`'s audit and the crate's own manifest:
    above a written file that holds a `Cargo.toml`, so a file outside any crate is never checked,
    and one batch gives one check per crate.
16. `splice` and `write` both read a file's formatter outcome and the language's own `Outcome`
    and `Check`; the answer borrows them there, never rebuilds them here.
17. `lang::table` is the one place a file's structure is read: `change`, `skeleton`, `fault`,
    `read`, `splice`, `grep` and `lang::graph` all ask it, so a change to what counts as a piece
    reaches every one of them.
18. `lang::shape` and a language's own relation index (`lang::relations`) are the decisions a
    consumer reads: a tool judges what a piece is through `shape`, and where a name is declared
    through the language's relations, never by cutting a label or a `::`/`.` itself.
19. A sorter answers a table and nothing else: display caps, sorting, folding and prose stay in the
    tool that shows them.

## Not wired yet

1. A `kairos` call in a sub-session fails at call time: a child has no job service, and the roster
   does not screen for it.
