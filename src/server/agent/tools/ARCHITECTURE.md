# tools — architecture

What is here, what each file holds, who talks to whom. Current state only.

## Files

| file | holds |
|---|---|
| `mod.rs` | module declarations and re-exports |
| `contract/mod.rs` | declares the five contract groups |
| `contract/call.rs` | `Batch`, `Cwd` |
| `contract/output.rs` | `ToolResult`, `ToolOutput`, `Effect`, `MemoOp`, `AliasOp` |
| `contract/env.rs` | `Env`, `Defaults`, `Limits`, `BashEnv`, `Paths`, `Seen`, `Stamp` |
| `contract/view.rs` | the `Source`, `Export`, `Child`, `Draft`, `MemoRead` ports, the history shapes, and `MemoScope`, `MemoReadOp`, `MemoVersion`, `MemoView` |
| `contract/job.rs` | the job vocabulary a tool shares with the loop: `JobSpec`, `Schedule`, `JobRunner`, `JobCtx`, `JobOutcome`, `Receipt`, `Row`, `Status`, `Exit`, `OutputSink`, and the `JobHost` port |
| `engine.rs` | `Engine`: `Batch::run` — env, gates, decode, run, stamp, measure (every tool but `read`), events; a call whose arguments arrived answers, and a stop landing mid-call becomes the result's reason |
| `spec.rs` | `Kind`, `Param`, `schema_of`, `Args`, `Item`, `Tool`, `Run`, `Summary`, `ToolProgress` |
| `summary.rs` | one recorded call's arguments, and the clip every tool's history summary reuses |
| `registry.rs` | `TOOLS`, `spec`, `roster` |
| `workspace.rs` | `Workspace`, the licensed zones, the per-path write lock, `write_atomic`, `canonicalize_lenient` |
| `bash/mod.rs` | declares the bash family |
| `bash/bash/mod.rs` | declares the command tool, the admission gate, the segment scanner, the guard and the runner |
| `bash/bash/tool.rs` | the `bash` tool |
| `bash/bash/admit.rs` | `admit`: aliases → `#ref` → guard, and the line it hands the runner |
| `bash/bash/segment.rs` | the separator set and the segment heads; the guard and the alias expansion share both |
| `bash/bash/guard.rs` | the command classifier: `normalize`, `classify`, `Verdict`, `RuleHit` |
| `bash/bash/shell.rs` | one command line: `run`, `run_to`, `OutputSink`, `Exit`, `StopFlag`; registers the stop action, spawns and signals through `platform::proc` |
| `bash/read/{mod,tool}.rs` | `read`: the path array, directories, the per-kind budgets and the per-file `limit` under them, the module view |
| `bash/read/sparse.rs` | the sparse anchor scan: comments and literals blanked, block edges numbered |
| `bash/read/hub.rs` | the module graph: entry files, exports, the edges inside one directory, the `pub use` index, the upward importer scan |
| `bash/read/dir.rs` | directory listings: one level, `src/` a section of its own, entry counts, junk named but never opened |
| `bash/grep/{mod,tool}.rs` | the `grep` tool: one query over the crate's `src/`, its three parameters, its summary |
| `bash/grep/scan.rs` | the range: nearest `Cargo.toml` above the cwd, the directories `scope` names, the `.rs` files under them read in parallel, the matcher, each file's hits |
| `bash/grep/rust.rs` | items from text: a header's ends by bracket depth, what a header binds, each item's container, a body's variants and fields |
| `bash/grep/outline.rs` | the folded answer's skeleton: one row per item at its span, the hit lines under the item that holds them, a file's whole top level once its hits spread over several |
| `bash/grep/render.rs` | the answer: the type, module and x-ray sections, hit tiers, every cap's own count line, the `[scope]` lines, and the stamps |
| `bash/write/{mod,tool}.rs` | `write`: the content through the formatter, the write, then the compiler's report |
| `bash/edit/{mod,tool}.rs` | `edit`: one unique run of text, with `exact_text` and the one-line receipt |
| `bash/splice/{mod,tool}.rs` | `splice`: whole lines by number, many ranges at once; the batch's calls on one file merged into one write; every edit's own verdict, the compiler's faults split onto the lines they hit; `mass_text` and `parse_range` for `final` |
| `bash/splice/edit.rs` | one change: its range, its block, the overlap rule, the reverse-order splice, where its lines land without a formatter, and the content search that finds a refused range in the file as it now stands |
| `bash/halt.rs` | the one sentence a stopped turn leaves: `INTERRUPTED`, and the answer a tool gives when nothing was written |
| `bash/gate.rs` | the compiler gate: the formatter's outcome for one text (remembered per edition and content), the edition of the crate a file belongs to, one `cargo check` of that crate with a deadline, every child in a process group of its own |
| `bash/skeleton.rs` | the file's skeleton from text: the items at each indent, each one's span, the innermost item holding a line, the seam drawn around a range |
| `bash/fault.rs` | the compiler's report, drawn: one block per error — the item it sits in, the lines around it, what the compiler said and suggested |
| `bash/splice/fmt.rs` | rustfmt through the gate: every change at once, then each one on its own so a refusal blames its own block, the change that breaks the company, and the comment walk that finds where each change landed |
| `bash/splice/receipt.rs` | the receipt: the verdict on the first line, every refused edit with the lines it takes and the block it would put there, every written edit one line at its landing and the item holding it, and the segment memo a front end draws |
| `bash/splice/items.rs` | items from text for the receipt's breadcrumb: a header's ends by bracket depth, each item's container, every item around one line |
| `bash/change.rs` | `context` (the item around a changed line, its breadcrumb and excerpt), `receipt`, `line_of`, `lines_of` |
| `bash/cd/{mod,tool}.rs` | `cd` |
| `bash/alias/{mod,tool}.rs` | `alias` add/list/remove; the config layer is read-only here |
| `bash/context/{mod,tool}.rs` | `context`: `query` through `Source::history`, `show` through `Source::block`, long listings to `Export::dir` |
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
| `memo/memo/{mod,tool}.rs` | the `memo` tool: writes as effects, reads through `MemoRead` |
| `memo/todo/{mod,tool}.rs` | the `todo` tool: reads the anchor, replays, applies, writes the memo |
| `memo/todo/replay.rs` | the pure replay: a call sequence → the list, anchored on the newest reset |
| `memo/todo/ops.rs` | the three todo operations (`set`, `tick`, `view`) and their batch apply |
| `memo/todo/state.rs` | `TodoItem`, `counts` |
| `subagent/mod.rs` | declarations and re-exports (`TASK`, `FINAL`), and `project_dir` |
| `subagent/task.rs` | the `task` tool: the call's project folder, template, variables, answer name, the `Child` effect |
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
      bash/*          ── shell, file tools, alias, context
      grep            ── scan (the walk) ── rust (sparse's blanker and frames)
      kairos          ── Source::jobs (a JobHost) ── loop/job
      task            ── template, answer name ── Effect::Child
      final           ── Source::child (a Child) ── draft, answer file
      mcp             ── mcp::sync ── mcp::stdio
      web, search     ── crate::web
      memo/todo       ── Source, and the pure todo replay
  → effect stamping → spill → ToolOutput
```

- `Env` is built once when a session is booked, from `Defaults`; the engine runs it as it stands.
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
15. `read`'s caps and `engine`: the front door skips `measure` for `read` because the tool cuts its
    own files; remove either half and a single-line file reaches the context whole.
16. `gate::check` and the crate's own manifest: the check runs in the nearest directory above the
    file that holds a `Cargo.toml`, so a file outside any crate is never checked.
17. `splice` and `write` both read a file's edition and formatter outcome from `gate`; the answer's
    own `Outcome` and `Check` are borrowed there, never rebuilt here.
18. `skeleton` and `fault`: the compiler's seam walks the same item tree the outline does, so a
    change to what counts as an item reaches both.
19. `splice::items` copies that tree rather than sharing it: the receipt needs the item a landing
    sits in and nothing else, and the two readerships move apart.

## Not wired yet

1. A `kairos` call in a sub-session fails at call time: a child has no job service, and the roster
   does not screen for it.
