# tools

The tool layer: one table of tools, one front door for a call, and one folder per family. A tool
is a function over `Env`: it reads what it needs from `Env`, returns a `ToolOutput`, and never
opens the database, the config file, or the loop.

## 1. Design / Schema

### The table

`registry::TOOLS`, in order:

| # | tool | file |
|---|---|---|
| 1 | `read` | `bash/read/tool.rs` |
| 2 | `grep` | `bash/grep/tool.rs` |
| 3 | `edit` | `bash/edit/tool.rs` |
| 4 | `splice` | `bash/splice/tool.rs` |
| 5 | `write` | `bash/write/tool.rs` |
| 6 | `cd` | `bash/cd/tool.rs` |
| 7 | `bash` | `bash/bash/tool.rs` |
| 8 | `alias` | `bash/alias/tool.rs` |
| 9 | `kairos` | `kairos/tool.rs` |
| 10 | `task` | `subagent/task.rs` |
| 11 | `final` | `subagent/final.rs` |
| 12 | `mcp` | `mcp/mod.rs` |
| 13 | `web` | `web/mod.rs` |
| 14 | `search` | `search/mod.rs` |
| 15 | `memo` | `memo/memo/tool.rs` |
| 16 | `todo` | `memo/todo/tool.rs` |
| 17 | `context` | `bash/context/tool.rs` |

**The session's list is the only gate.** `roster(names)` expands a list into manuals, keeping the
rows whose names appear in it and nothing else. `engine` reads the same list through
`env.allows(name)`, so a tool absent from the list cannot be called by naming it. No tool has a
second condition of its own: whether a session gets `mcp` depends on the profile naming it, not on
a server being configured locally.

Two names are settled when a child is spawned, in `loop/turn/cycle.rs`:

- `task` is dropped from every child's list. A child never spawns a child.
- `final` is appended to every child's list. The parent does not name it and does not know it
  exists.

`web` drives the session's work tab, so its operations sit in separate calls; `search` holds the
session's search tab and names nothing else.

### One call

```text
a batch with the turn's stop signal
  → stop already set: the call never runs, and answers that it never ran
  → registry::spec(name)
  → env.allows(name)                       the session's list; the only gate
  → budget gate (every call but `final`)
  → Args::fill(tool.params, args)          fields → positional values
  → (tool.run)(env, &args, progress)       Start, Progress and End events around it
  → a stop that landed meanwhile makes the call failed, reason `interrupted`
  → every out.effects gets one ts
  → spill: over the inline budget, the full text moves to out.spill (`read` excepted: it bounds itself)
  → ToolResult
```

Unknown names error.

### Schema

A tool declares `params: &[Param]`; `Param { name, kind, default, about }` and
`Kind = Text | Number | Line | Flag | Object | Choice(&[str]) | TextList | Numbers | Objects(&[Param])`.

Every call is one flat object: `intent` beside the tool's own fields, all at one level. `schema_of`
adds `intent` once for every tool, so no tool declares it.

`Objects` is an array of objects, and its items are the one place a field is read by name: a
position indexes a call's top level only. Its own `Param` table is rendered under the array's
`items`, and `Args::objects` hands back one `Item` per element, taking defaults, kinds and enums
from that same table. An item error names itself, `files[2].path`.

- `about` says what the tool does and what it answers. No field list.
- `fields` says what the fields are and which ones a call needs per `type`. No prose the schema
  already carries: a field's own `about` says what its value is.
- Each field says what its value is; its type and enum come from the table.
- Only `intent` is `required`: which other fields are needed depends on `type`, and only the tool
  knows that. `Args::fill` reads the fields by table and fills defaults; a tool that needs one
  errors where it reads it, naming the field.
- A name the table does not hold is refused by `Args::fill` before the tool runs, and an object
  array refuses one of its items' unknown fields the same way: a call that names `range` to `edit`
  hears back the names `edit` takes.
- The front door refuses a call whose `intent` is missing or blank.

Every field but `intent` is optional in the schema; `always` below marks the ones every call of
that tool needs (the tool checks them, not the schema).

| tool | parameters, in index order |
|---|---|
| `grep` | `query` always, `regex` (default false), `scope` (directories under the crate's `src/`, relative to the crate root; omitted, the whole `src/`) |
| `read` | `files` always: one item per path, with its own `offset` (1-based, default 1) and `limit` (omitted: 800 for a `.rs` file, 200 otherwise; a larger value is clamped to that cap); a Rust file gets 800 lines and 256 KiB, anything else 200 lines and 64 KiB |
| `edit` | `path` always, `old` always (one unique run), `new` always |
| `splice` | `path` always, `edits` always: one item per range, with `start` and `end` (both 1-based, inclusive; `end = start - 1` inserts) and `new` |
| `write` | `path` always, `content` always |
| `cd` | `path` always |
| `bash` | `command` always |
| `alias` | `type` (`add`/`list`/`del`), `aliases` (type=add: one item per alias, `name` and `command`), `names` (type=del) |
| `kairos` | `type` (`now`/`after`/`every`/`list`/`status`/`result`/`cancel`), `tasks` (type=now/after/every: one item per task, with `content`, `kind` (`command`/`text`, default `command`), `timeout`, `delay`, `expr`, `count` (default 3)), `id` |
| `task` | `tasks` always: one item per child, with `template`, `vars` (object of strings), `cwd`, `maxToolCalls`, `name` |
| `final` | `type` (`write`/`edit`/`read`/`submit`), `content`, `old`, `new`, `range` |
| `mcp` | `type` (`list`/`call`), `calls` (type=call: one item per call, `server`, `tool`, `args` (object)), `server` (type=list) |
| `web` | `type` (`read`/`open`/`click`/`fill`/`select`/`scroll`/`eval`/`screenshot`), `url`, `raw` (default false), `selector`, `value`, `pixels` (default 600), `full_page` (default false) |
| `search` | `query` always, `provider` always, `pages` (default 1) |
| `memo` | `type` (`add`/`mod`/`del`/`show`/`history`/`list`), `scope` (`current`/`global`, default `current`), `name`, `content` |
| `todo` | `type` (`set`/`tick`/`view`), `items` (the whole list), `ids` (item numbers, 1-based), `done` (default true) |
| `context` | `type` (`query`/`show`), `anchor` (a `#<ref>` hash or a block id), `direction` (`before`/`after`, default `after`), `limit` (default 3), `tool` (a name or `all`, default `bash`), `ids` |

`provider` is `search`'s alone: one search runs against exactly one provider, so a call that names
none is refused rather than spread across several tabs.

`cd` lands its result with `memo {kind: "cwd", path, from}`: the history card relativizes from it
and the front end marks the directory as a temporary move.

### Batched calls

Five tools take an array where they used to take one item: `read`, `alias`, `kairos`, `mcp` and
`task`. `edit`, `write`, `memo`, `context`, `final` and `web` stay single, because their
payloads are large enough that a batch would only move the same bytes into one argument. `splice`
takes an array for a different reason: its items are ranges of one file, applied to one snapshot,
so a batch is one write instead of several that would each need the file read again.

- Items run in the array's order, one after another; the answer is laid out in the same order.
- Items are isolated: one item's failure is written where that item would have answered, and the
  rest still run and still answer. The call fails when any item failed, and `fail` counts them.
- Every item states its own outcome in `items`, aligned with the arguments' order: the front end
- `read` budgets belong to each path, not to the call: a Rust file gets its own 800-line and
  256 KiB allowance, anything else 200 lines and 64 KiB; `limit` bounds the window below that
  allowance and never above it; a line over the byte cap is cut where the characters still fit,
  and that file's status line names the byte cap with no offset.
- A query operation — `alias list`, `kairos list/status/result/cancel`, `mcp list` — takes no
  array and refuses one.
- A batch whose items produce structured payloads answers a memo array, and the history cards
  draw one entry per item.
- A call with exactly one item answers as the single-item call always did: its one item's failure
  is the call's failure, with no item label in the text.
- `task` allocates every answer name against the ones the same call already handed out, so two
  children with one stem get `probe` and `probe-1`, not the same file.

### ToolOutput

```text
ToolOutput { text, items: Vec<ItemOk>, fail: Option<String>, memo: Option<Value>, effects: Vec<Effect>, stop: bool }
ToolResult { call_id, items: Vec<ItemOk>, result, reason: Option<String>, memo, spill, duration_ms, effects, stop }
Spill      { name: String, content: String }
ItemOk     { ok: bool, error: Option<String> }
```

- `text` is what the model reads; `memo` is the structured payload the front end renders.
- `items` is what each item of the call answered, in the arguments' order, and it is the only
  source of the front end's status: the card draws one row per item and hangs that item's `error`
  under it. A single-item tool leaves it empty and the front door answers one entry.
- `fail` is why a call was cut short; any value fails the call. The front door moves it to `reason`
  and appends `[reason]` after the body and its artifact note, so the marker is the result's last
  line whatever the truncation did. A cut call states its reason in its one item.
- `spill` is filled by the front door, never by a tool.
- `effects` are side effects that must land with this tool result. A tool appends one; the front
  door stamps its `ts` before returning.
- `stop` asks the loop to end the round.

### Contract

`contract/` groups the tool layer's outward shape by what a caller reaches for. No name is
re-exported at the `tools` root; callers write the group they mean.

| module | holds |
|---|---|
| `contract::call` | `Batch`, `Cwd` |
| `contract::output` | `ToolResult`, `ToolOutput`, `Effect`, `MemoOp`, `AliasOp` |
| `contract::env` | `Env`, `Defaults`, `Limits`, `BashEnv`, `Paths`, `Seen`, `Stamp`, `Answer` |
| `contract::view` | `Source`, `Export`, `Child`, `Draft`, `MemoRead`, `MemoScope`, `MemoReadOp`, `MemoVersion`, `MemoView`, and the history shapes `CallRecord`, `BlockPair`, `Direction` |
| `contract::job` | `JobSpec`, `Schedule`, `JobHost`, `JobRunner`, `JobCtx`, `JobOutcome`, `Receipt`, `Row`, `Status`, `Exit`, `OutputSink` |

### Env

```text
Env {
  workspace: Workspace,      // the session's shared cwd handle
  limits: Limits,            // lines, bytes
  bash: BashEnv,             // timeout, shell
  stop: Arc<Stop>,           // the session's stop signal; see Stopping
  session: i64,
  batch_len: usize,
  tools: BTreeSet<String>,   // the session's list; the one gate
  aliases: Aliases,          // config layer above the session layer
  default_budget: u32,
  export: Arc<dyn Export>,   // a full text out of the database as a file
  paths: Paths,              // the user's templates; the project's folders come from the live cwd
  source: Arc<dyn Source>,   // reads the loop owns
  seen: Seen,                // the module cards shown, and every file's read stamp
  batch: Vec<ToolCall>,      // the calls now running, so one can merge with another in the same batch
  current: String,           // the call now running
  handed: BTreeMap<String, Answer>, // what a merged run already answered for a later call in the batch
}
```

`batch`, `current` and `handed` are per batch, not per session: the front door fills them before
the first call and clears the ledger with it. A tool that merges work (only `splice` does) answers
for the other calls once and hands each its own result when its turn comes.

Ports are traits in `contract::view`: `Source` (memos, aliases, history, calls, jobs, child, and
the MCP bridge), `Export` and `Child`. A tool that needs a port fails when it is `None`; it never
reaches for a database or a config file.

`Env` carries no browser settings. The browser decides how it starts; see `src/web/README.md`.

### Workspace

```text
Workspace { shared }
```

- `cwd` reads the handle as it stands; `chdir` writes it. The api's `/cdp` and the model's `cd`
  write the same handle, so a move lands for both at once.
- `resolve` licenses two zones only: the live `cwd` and the temp directory. A path outside both is
  refused; `must_exist` adds the file checks.
- `cd` is licensed by one rule, `may_stand_in`: not the filesystem root, not a top-level directory
  (`/tmp` and its children excepted), and nowhere under the home tree. Everything deeper is fair
  ground, and the licensed zones then follow `cwd`.
- Where a session may stand is asked in two places only: `init` and `bind` land a barred or
  missing launch directory in the temp dir (`standing_dir`: it must exist, be a directory, and pass
  `may_stand_in`; a row whose directory has since gone lands there too), and `cd` refuses it
  outright. `/cdp` asks only whether the directory exists.
- `with_write(path, f)` takes a process-wide write lock keyed by that path; `write_atomic` writes a
  temp file beside the target and renames it into place. `read` takes no lock.
- `bash` takes no lock.

### Memo and todo

- `memo` writes (`add`, `mod`, `del`) are `current` only; a `global` write is refused. Reads
  (`show`, `history`, `list`) take both scopes.
- A memo write returns placeholder text and one `Memo` effect; the loop lands it in the same
  commit as the result block, so the tool itself never stores a note.
- `memo` reads go through `Source::memo`; the tool never opens the database.
- Three operations: `set` replaces the list (`items: []` clears it), `tick` marks item numbers done
  or, with `done: false`, clears the tick, and `view` prints the list with those numbers. Numbers
  are positions; an out-of-range number drops the whole call and nothing changes.
- A todo result's memo carries the whole list, so a front end maintains it by replacement. A
  fully-ticked list stays until the next `set`; nothing clears itself.

### Bash order

```text
admit:  expand_alias  →  rewrite #<ref>  →  guard::classify     then  shell::run
```

`admit` is the only way raw text becomes a spawnable line; `bash` and `kairos` both call it. `guard`
sees the final command. It returns `Allow`, `Warn(ids)` or `Block(ids)`; a blocked command never
starts. `kairos` admits at tool-call time and freezes the admitted line, so a job exists only for a
command that already passed. `alias add` runs the same classifier on the command it stores.

One call may carry several lines; the shell runs them in order. A bare newline is a separator to the
guard, a backslash before a newline is a continuation, and `expand_alias` rewrites the head word of
every segment once.

The command itself runs in the session's shell (`Env.bash.shell`): `bash` or `zsh` on unix, `pwsh`
or `niu` on windows. `platform::shell` builds the command line and the quoted path a `#<ref>` becomes;
`platform::proc` starts it as its own process group and stops it group-wide, TERM then KILL. A stop
arrives through the `Stop` the runner registered after the spawn; the tool thread itself only
receives, so nothing wakes it on a timer. Stdin is the null device: a command that prompts reads
EOF, never the terminal the front end owns. The guard belongs to the unix shells only; `pwsh` and
`niu` run the line as written.

### context

- `query` needs `anchor` and `direction`; `tool` is one name or `all`. `anchor` takes a `#<ref>`
  (the hash printed when a result was truncated) or a block id. The answer is a JSON array of
  `{id, tool, ok, summary}`, one element per recorded call.
- `show` answers a JSON array of `{id, request, result}`; request and result are the stored text
  parsed back into JSON, `null` when the block carries none.
- Past ten rows, or past three shown blocks, the text lands in `Export::dir()` (`<temp>/mypi`) and
  the reply names the file. The name carries the session and the parameters, so the same query in
  one session overwrites its own file and no two sessions collide. The memo still carries every row.

### Query answers and summaries

The tools that read history answer JSON, and the same value rides along as the memo: `context`
`query`/`show`, `memo` `show`/`history`/`list`, `alias` `list`, `kairos`
`now`/`after`/`every`/`list`/`status`, `mcp` `list`. A starting `kairos` operation answers one
receipt per task, so its memo is an array. The front end draws what the memo carries and does not
parse the text.

Every tool declares `summary`, one line that names the call in a history listing; the field is not
optional, so no tool joins the table without one. It carries only the least that locates the call:
the first path plus a count for `read`, the path for `write`, `edit`, `splice` and `cd`, the truncated
command for `bash`, the first name and command for `alias`, the operation for the tools whose
parameters are the interesting part (`memo`, `todo`, `context`, `kairos`, `final`), the first
server and tool for `mcp`, the first template and its answer name for `task`, the query for
`search`, and for `web` the operation plus the argument it names: URL, selector or expression.
A tool name the table no longer holds shows its clipped arguments.

### read

`read` answers one array of paths: a file comes back as its body, a directory as its entries.

A file's body carries line numbers only where a change can start or end. `splice` locates a change
by line, so a session with `splice` numbers a Rust file at each block's first and last line: a
declaration, a run of `use` statements, a hat of attributes and comments (its first and its last line,
however many it stacks; the file's own head counts even when a blank line follows it), the closing brace of any numbered block, and — inside a function
longer than fifteen lines — every block head: `if`, `else`, `match`, `while`, `for`, `loop`,
`unsafe` and `async`, whether it stands on its own, follows a `}`, or opens behind `let x =`, an
assignment, `return` or `break`. A `let … else` counts, and so does a macro call or a closure whose
`{` ends the line. No head needs its `{` on that line: a folded condition puts it three lines down. Every other line
stays unnumbered, so every numbered block has a `(start, end)` a call can name without counting.
Other languages keep a number on every line: only Rust is read this way.

`edit` locates a change by text, so a session with `edit` and without `splice` gets no numbers at
all: the body is the file, ready to be copied into `old`.

The gutter is as wide as the file's own last line number plus one pipe, so a three-digit file
writes `123|` and an unnumbered line writes `   |`: the pipe keeps the body in one column and makes
the boundary between gutter and code visible. An empty line is a newline and nothing else. The
gutter's bytes are part of the body's budget.

A status line opens the body when the file continues: `[800 of 900 lines; next offset 801]`, or
`[256 KiB cap; 161 of 200 lines; next offset 162]` when bytes ran out first. A line longer than the
room left is cut at a character boundary and not counted, so the next read starts at that same
line. A file that ended, and a window that starts past its end, answer no status line at all.

A directory answers one level: `name  N lines` for a file, `name/  K files, F dirs` for a
directory. Inside `src/` each child directory is opened one level too, folding to counts at ten
files. The session root lists its own entries, then `src/` as a section of its own with `src/`'s
children beneath it. Build junk — `target`, `node_modules`, dot entries — is named like anything
else and never opened. A listing past 800 lines ends with `[+N entries]`.

### The module view

A Rust file read inside a crate arrives with its module's context, because a module is edited as a
whole far more often than a file is. The view follows the body, opens with
`--- module a::b (src/a/b/) · primer ---`, and holds four parts:

- `exports (N):` — the entry file's `pub use` statements, or its `pub` items when it forwards none.
- `down (N):` — the modules the entry declares, one line each: label, size (files for a directory,
  lines for a leaf), visibility, `hub` or `leaf`, its exports, and the modules of its own directory
  it reaches into.
- `up (N):` — the entry that declares this module, then every file outside the module that imports
  it, with the line and the symbols it takes.
- One `[scan: up→… | down→…]` line naming where each search stopped.

The tag on the opening line says why the view is there: `primer` when it rode along on the first
read in a module directory, `entry file` when the entry file itself was read. Each directory gets
one primer per conversation, and a compaction clears that memory. A file this same call already
answered shows `[body in this batch]` instead of its exports.

A consumer writing a forwarded name is followed through the `pub use` on the way up, and the hit
says `via src/a/b/mod.rs`. `pub use x::*` cannot be followed and is skipped; `include!`, `#[path]`
and `cfg` are not followed either. A line over 400 characters ends with `[+N]`, and a list cut
short ends with `[+N more]`.

The view has its own 800-line and 256 KiB budget, apart from the body's: a body that fills its cap
still comes with a whole view. The upward walk covers the next directory up and everything beside
it, one whole level at a time, and stops at the crate root or when its 100 ms budget runs out. A
level it could not finish is dropped whole, because half a level reads as "nobody imports this".
The downward side never walks: it reads the names the entry declares and nothing inside them.

### grep

`grep` answers one query over the crate's own `src/`. The crate is the nearest `Cargo.toml` above
the session's directory, and only `.rs` files under its `src/` are read: no ignore rules and no
external binary, so the answer is the same on every platform. `target/` is out of reach by
construction. Anything outside `src/` — `Cargo.toml`, `build.rs`, the root documents — is one
`read` away and never searched. `scope` narrows the walk to named directories under `src/`, and the
files of one answer are read in parallel. `query` stands for
itself unless `regex` is on, when it is a pattern; a pattern that does not compile is refused with
the engine's own position and a reminder that the switch is off by default.

Structure comes from the passes `read` already runs: comments and literals blanked, braces paired,
head words recognized. A header runs to its own `{` or `;` with `(`, `[` and angle brackets
counted, so a wrapped signature, a `where` clause and nested generics stay inside one header, and
`->` contributes no closing angle. An `impl` type is the text after `for`, or after `impl` when
there is none, with `&`, `&mut`, `!`, lifetimes, `dyn` and generics stripped.

Three sections precede the hits:

- `[type]` — `A::b` where `A` is declared here: the `b` inside `impl A`, `impl Tr for A`, `trait
  A`, `impl Tr for A` on a trait's behalf, and the variants or fields of `enum A` and `struct A`.
  A name this crate does not declare says so in one line, and the hits still follow, so a
  standard-library type never answers "nothing".
- `[module]` — `A::b` where `A` is a `mod`: the item `b` the module's own files declare. The module
  is found through its `mod a;` declaration, never by guessing a directory from the session's path.
- `[x-ray]` — when the query is a lone `struct`, `enum` or `trait` name: its declaration, then
  every `impl` block for it across the crate, each with its members' spans. It is a map, not a hit
  list, and it has two caps of its own (`xrayImpls`, `xrayItems`) that take no share of the hit
  caps.

Hits are answered at the density their count allows, by `deepMax` and `foldMax`. Up to `deepMax`,
each hit comes with its innermost item whole — or a six-line window when that item is longer than
forty lines — under an `in:` breadcrumb of every item around it. An excerpt line carries its
number only when it is a hit or an anchor of `read`'s own sparse scan; every other line keeps the
gutter blank, so a number means a line `splice` may start or end on. Up to `foldMax`, each file
answers as its skeleton: one row per item, the hit lines under the item that holds them, and a
file whose hits fall in more than one item answers as its whole top level with those items opened.
Past that, only per-file counts and a line saying the query is too wide. `fileMax` caps one file's
hits in one answer. Every cap that bites states what it left out; nothing is dropped silently.

`scope` narrows the search to directories under the crate's `src/`, named relative to the crate
root. A directory that is missing, is not a directory, or stands outside `src/` is skipped while
the rest still run, and the answer ends with one `[scope]` line per such entry naming the reason.
Every file whose lines an answer showed is stamped in `env.seen` — line count and content hash, the
stamp `read` writes — so `splice` works on it without a second `read`.

`grep` bounds its own text, so the front door does not measure it: what it answers is what the model
reads, however long that is.

What it does not read: anything a macro generates, the `impl`s a derive writes, `include!`,
`#[path]`, and a type renamed by `use … as`. A hit inside such code is still a hit; only its
breadcrumb is coarser.

### edit and splice

`edit` needs `old` (one unique run) and `new`. `splice` needs `edits`: one item per range, each
with `start`, `end` and `new`; `new` may be empty to remove the range, and `end = start - 1`
inserts instead of replacing. A field the table does not hold is refused, so `range` in an `edit`
call and `old` in a `splice` call both fail on the name.

A session gets one of the two, because two ways of locating one change pull the model apart. A
profile's `tools.yaml` may name either; naming both keeps `splice` and drops `edit`, with a notice.
A profile that names no list gets every tool but `edit` and `final`.

### write

`write` overwrites a file whole, and refuses nothing: it is the counterpart of `edit`, which refuses
an ambiguous match. A `.rs` file goes through the formatter first, and the formatted text is what
lands; a text the parser refuses still lands as it came, so a long file may be written in parts, and
the answer says the file does not parse yet and skips the type check.

A written file is type-checked the same way a spliced one is, and the same report follows the answer.
A stop before the write leaves the file untouched and the answer says so; one after it skips the check.
Nothing is said when there is nothing wrong, so a clean write answers in one line:

```text
overwrote /p/src/foo.rs (2311 bytes)
```

### edit

`edit` answers one line naming what went, what took its place, and how far the rest moved:

```text
replaced lines 12-14 (3) with 5 lines; below +2; file now 144 lines: /p/src/foo.rs
```

### splice

A Rust file goes through rustfmt, and the formatting is also the gate: a text the parser refuses is
never written. The whole file is formatted, not the changed lines, so every line number in the
answer is a line number on disk.

The ranges are applied to one snapshot, from the bottom up, so the lines above keep their numbers
while they are still being used. Two ranges of one call may not overlap; that is the model
contradicting itself and the file is left alone. Every `splice` call on the same file in one message
is one write: the calls are merged in arrival order, and a range that overlaps one from an earlier
call is refused while the rest still land.

One pass applies everything. When the file does not parse, the changes are re-applied from the file
as it was, one station at a time from the bottom up: each station adds one change and the whole file
is parsed again; the stations that parse stay, and the one that does not is left out with its own
error. That is how a broken range is named without guessing from an error line, which a missing
brace would put anywhere below it.

Every parse is one `rustfmt` run over stdin. The binary is looked for beside this executable first,
then on PATH, and its edition comes from the nearest `Cargo.toml` above the file, read once per
directory. A missing rustfmt refuses the write rather than skipping the format.

The formatter's answer for one text is remembered per edition and content, so a retry of a text the
session has already formatted costs no process; a run the user stopped is not remembered, because its
answer came from a kill and not from the parser. Every child the gate starts leads a process group
of its own and is killed with that group when its deadline passes or the turn is stopped, so a
formatter can never hold the call that started it.

Anything that is not a `.rs` file is spliced without a parse and without a format: there is no
formatter to call, and no line numbers are trusted beyond the ones `read` gave.

`splice` refuses a file that changed on disk after `read` described it, and it refuses it first:
the session remembers every file's line count and content hash, and the call's first act after
reading the bytes is to compare them. A mismatch ends the call there — no range is parsed, nothing
is formatted — and the model is sent back for a fresh read instead of writing a range at
coordinates that moved.

#### splice's answer

The answer carries the file's skeleton at its new line numbers, then the seam each change sits in:

```text
[Splice OK] 2 edit(s) applied, formatted by rustfmt; 24 lines: /p/src/foo.rs
--- Skeleton (post-format line numbers) ---
   3..  3 | use std::fmt;
   5..  7 | pub struct Greeter
   9.. 17 | impl Greeter <- modified
      10.. 12 | pub fn new() -> Self <- edit 1
      14.. 16 | pub fn greet(&self)
  19.. 21 | pub fn main()
--- edit 1 · was line 11 · now 11..11 ---
```

The change's own top-level block is expanded to its items; every other block is one line. Seams
show two lines above and two below, with a long change folded in the middle. The whole outline is
capped so an answer stays small; the seams stop when the cap is reached.

A change left out is expanded instead: its range, the line of its own block that broke, what the
parser said, and the block exactly as the model wrote it. The calls of one file answer in arrival
order, and only the one that ran the write carries the skeleton; the others carry their own ranges
and refusals.

A stop lands in one of two places, and the answer says which. Before the write, the formatter is
killed, nothing reaches the file, and the call answers that the file is unchanged. After the write,
the change stands and the type check is skipped, so the answer says the file was written and not
checked.

The compiler gets the last word. Once the write is on disk and the file's lock is released, the crate
the file belongs to is type-checked, and what came back is appended to the answer of the call that
ran the write — only when there is something to say. Errors in the changed file come first, every
other file's after; each carries the item it sits in, the lines around it, the compiler's message
and its suggestion. The check is bounded: a crate still compiling when the deadline comes is dropped
and the answer says so, rather than holding the call.

```text
[Splice Partial] 2/3 edits applied, 1 left out, formatted by rustfmt; 21 lines: /p/src/foo.rs
--- edit 2 left out · was line 14 ---
your block, line 2: expected `;`, found keyword `self`
   1 |     pub fn three(&self) -> u8 {
   2 |         self.x + 2
```

### edit's and splice's memo

```text
edit:   { "atLine": 12, "scope": ["impl Greeter"], "excerpt": { "from": 8, "lines": [...] } }
splice: { "segments": [ { "edit": 1, "state": "applied", "fromLine": 11, "atLine": 11,
                          "old": "<lines that went>", "new": "<lines that took their place>" }, … ] }
```

`edit`'s memo describes one change: `atLine` is the old-file line it starts on, `scope` the
breadcrumb of the blocks above the item, `excerpt` the item as it stood before the change — its
first old-file line and every line of it — or a window around the change: five lines above it and
five below, counted over the lines the change covers. The window is taken when the item does not
hold the whole change, and when the item is longer than
`config.server.agent.tools.edit.fullItembudget` lines (default 40), because a card folds a long body
and the fold would hide the change. A window is not the item, so the item's own declaration joins
the breadcrumb.

The item is the nearest enclosing block that declares something; failing that, the nearest one that
starts after a blank line, a comment or an attribute; failing that, the nearest block. Blank lines
never end an item, and an item's own closing brace belongs to it. The front end renders the excerpt,
signs the changed lines, and paints the rest.

`splice`'s memo is one segment per range, in the call's order: the old lines it took and the lines
that stand there now, numbered on both sides. A range left out carries its block and why instead.
Segments that are not neighbours are drawn apart, and the card folds what it does not need.

`edit`'s new line numbers, counts and the shift after the change are the front end's arithmetic.
`splice`'s are not: the formatter moved them, so the tool carries them in the memo.

### Stopping

`Stop` lives in `platform::stop`: a flag plus a table of actions. `set` sets the flag and runs every
action once, on the caller's thread; `on_stop` registers an action and runs it at once if the
signal already fired; the returned `Hook` takes it off again when dropped. `reset` clears the flag
at the start of a turn.

A tool that blocks registers what stopping it means: `bash` TERMs its process group and KILLs it
after `platform::proc`'s grace period; the gate kills the formatter or the compiler it started; `mcp`
kills the group of the server a call waits on, and the next call starts that server again. A tool
that finishes in microseconds registers nothing, and the front door does not pretend otherwise.

What a stop leaves behind depends on how far the call got:

| call | history |
|---|---|
| started | one failed result, `[interrupted: the user stopped the turn]` as its last line |
| stopped before it wrote | one failed result naming what is still untouched |
| never started | one failed result saying it never ran |

A reply cut short mid-stream keeps no calls at all: the model never finished asking, so no call ever
existed. Once its arguments arrive whole a call belongs to the turn, and every one of them answers —
the ones that ran with what they produced, the ones the stop kept from running with the fact that
they never ran. A stop ends the work, never the record of it. A call's own deadline is not a stop:
`bash` reports `timed out after Ns` through `ToolOutput::failure`.

`final submit` is not a stop either: its result carries `stop`, and the turn settles as `finished`.

### MCP

`mcp` is one static tool with a fixed schema. Which servers exist and which tools each exposes is
decided at runtime, so a new server needs no Rust change — only a line in `mcp.json`.

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
`rescan(cwd)`, `shutdown()`. Nothing above it knows JSON-RPC. `SyncMcpBridge` is the built-in implementation: one
child process per name, started lazily and kept until a rescan; a server spawns in the session's
working directory, so a `cwd` key in the file is not used. stdout is one JSON-RPC object per line,
a reader thread dispatches by id, stderr is drained into a 20-line tail that only surfaces in
errors; every call carries a deadline; a dead process fails every waiting call and is started again
on the next one. Every server leads a process group of its own, and a stop kills that group: the wait
ends on the same tick, and the failure a dead server gives is the call's answer. It uses threads and
channels only — no async runtime enters this tree.

An answer is text content, concatenated in order. `isError` makes the call failed. Images, audio
and structured content are reported as unsupported. MCP's own schema is not passed to the model and
not checked here; a bad argument is the server's error to report.

### Jobs and children

- A job is created by the loop, never by a tool. The tool writes a `contract::job::JobSpec` and
  holds the session's `JobHost`; the scheduler, the pool, the per-run files and the wakes live in
  `loop/job`. A session's service is built the first time anything asks for it, so a session that
  never runs a job keeps no scheduler and no output folder.
- One job, one completion: a command finishes, a child submits, or the clock arrives. The receipt
  names the job; the wake reports it done under the same id.
- `kairos` is the tool face. A text job has no completion of its own, so it refuses `now`.
- A child is a job too: `task` resolves a template and picks an answer name, the loop writes the
  session row, and a loop-side runner books the child, nudges it three times when it ends without a
  submit, then drops it.
- The project folder is `<the parent's cwd>/.mypi`, taken where the `task` call is made: a child's
  own `cwd` never moves it, and a `cd` before the call does. Nothing walks up to a parent folder.
- Templates come from `<project>/agents`, then the user's global agents folder, then the path the
  call names.
- The answer file is `<project>/answers/<name>.md`, created when the first child of that project is
  spawned; the name is fixed at spawn, from `name` or the template's stem, with `-1`, `-2` when a
  file or another item of the same call already holds it. `final submit` writes it and is the only
  thing that ends a child.
- A child takes no user messages. `task` is the only tool a child cannot hold; `final` is the only
  one it never has to ask for. A main session never resolves `final` at all: `init` drops it from
  the profile's list and from the manuals.

### Invariants

1. A tool reads only `Env`; no database, no config file, no loop.
2. A tool never measures or trims output; the front door does — `read` and `grep` alone bound their
   own text, which is why the front door does not measure them. `grep` caps its own hit lines by the
   same line budget and states every cap that bites, so nothing is spilled or dropped silently.
3. A tool names an effect kind; the front door stamps `call_id` and `ts`.
4. File tools read and write only inside the licensed zones; atomically, under a per-path lock.
5. The roster is the only way the model learns a tool exists.
6. `edit` has one path in, one read, one atomic write; neither `edit` nor `write` takes an array.
7. `alias` expands the head word of every command segment, once, and adds no variables, functions or
   multi-line bodies. A name a session writes is one word the scanner accepts as a segment head; a
   name the config fixes is read-only for the session.
8. One call runs at a time inside a session; different sessions run concurrently.
9. A tool starts no job and writes no session row; it describes both and the loop acts.
10. The session's list is the whole gate; a tool adds no condition of its own.
11. A stop is one `Stop`, and only a tool that blocks registers what stopping it means: `bash`, the
    gate, and `mcp`.
12. A call whose arguments arrived whole answers, whether or not it ran.

## 2. Design intent

- **One table, one front door.** Gates, argument decoding, effect stamping and output measurement
  happen once, so a new tool cannot forget one of them and cannot report output straight into the
  conversation.
- **Grouped contract.** The outward shape sits in five modules by caller intent, so a reader looking
  for "what a tool returns" does not scroll past the ports the loop implements.
- **The list decides.** A profile names what a session may call; the tool layer does not second-guess
  it. The one exception is settled at spawn, not at call time: a child's list is corrected once, so
  no check has to be repeated on every call.
- **Positional parameters.** The schema and the parser come from one `&[Param]`, so the JSON schema
  the model sees cannot drift from what the tool reads.
- **`intent` everywhere.** Every call carries one line of user-facing intention; the front end can
  show it without parsing arguments.
- **Ports, not dependencies.** Reads the tool layer must not own — memos, artifacts, jobs,
  children — arrive inside `Env` as traits, so the tools keep no database handle.
- **Effects land with their block.** A memo, alias or sub-session cannot exist without the tool
  result that caused it; one transaction owns both.
- **A guard, not a sandbox.** The model runs real shell commands; the classifier blocks the
  machine-destroying shapes and warns on the rest, and it always judges the expanded command.
- **Two questions, two rules.** File tools may touch the session's directory and the temp
  directory; `cd` answers a different one — where may a session stand at all — and its rule is a
  blacklist, not a zone list. The write lock is per path, so unrelated writes never queue behind
  each other.
- **Long work leaves the round.** `kairos` starts a job and returns; a finished job wakes the
  session, so a slow command does not hold a round open.
- **A child is a session.** Subagents reuse the loop, storage and stream; corrected lists keep a
  child from spawning children of its own.
- **One stdio server per name.** MCP servers start lazily and stay up for the process; the tool
  only knows names, so a new server is a line of JSON.
- **The browser is optional at build time.** `web` is a feature, not a runtime switch, because the
  browser stack is the largest dependency in the crate.

## 3. Unresolved

1. MCP carries text content only; a server-initiated request is answered `-32601`.
2. Template front matter reads `tools` only.

## 4. Config fields

| key | use |
|---|---|
| `config.server.agent.tools.bashTimeout` | one shell command's deadline; default 600 s |
| `config.server.agent.tools.aliases` | user aliases as `name=command`; every session reads them, nothing shadows them |
| `config.server.agent.tools.edit.fullItembudget` | lines an edited item may take and still be handed over whole, default 40; a longer item hands over the window around the change |
| `config.server.agent.tools.grep.*` | `deepMax` 6, `foldMax` 25, `fileMax` 6: the three hit thresholds. `xrayImpls` 8, `xrayItems` 12: the x-ray's own caps, which the hit thresholds do not charge |
| `config.server.agent.tools.outputMaxLines` | inline line budget, default 120; `read` and `grep` keep their own |
| `config.server.agent.tools.outputMaxBytes` | inline byte budget, default 32768; `read` and `grep` keep their own |
| `config.server.agent.tools.browser.bin` | the browser executable; empty = `$MYPI_BROWSER_BIN`, else `/opt/helium/helium` |
| `config.server.agent.tools.browser.profileDir` | the persistent profile; empty = `<data dir>/browser/profile` |
| `config.server.agent.tools.browser.port` | attach to a running debugging port instead of launching |
| `config.server.agent.tools.browser.idleSecs` | seconds without activity before an owned browser is closed; default 600 |
| `config.server.agent.tools.browser.userAgent` | override the launched browser's User-Agent; unset = the browser's own |
| `config.server.agent.tools.browser.extraArgs` | extra browser argv |
| `config.web.bringToFront` | raise the window when a verification challenge appears; default true |
| `config.server.agent.tools.kairos.*` | `maxConcurrent`, `timeoutSecs`, `maxOutputBytes`, `maxRetained`; the job scheduler's numbers, read by the loop |
| `config.server.agent.subagent.maxToolCalls` | one child's tool-call budget; read by the loop |
| `config.server.agent.loop.tools.task.timeoutSecs` | how long one sub-session may run; default 3600 s |

The browser keys are read by the web domain itself, not by the tool layer.
