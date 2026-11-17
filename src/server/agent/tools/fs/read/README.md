# read

One path per item: a file with its line numbers, a directory with its entries, a Rust file with its
module's context. The tool bounds its own answer, which is why the front door never measures it.

## 1. Design / Schema

### One call

`files` is one item per path, each with its own `offset` (1-based, default 1) and `limit`. Items are
answered in the order the call names them, and one item's failure never stops the ones after it. A
session with one item answers that item's error as the call's own.

A file's budget comes from its kind; `limit` narrows the line count and never widens it.

| kind | lines | bytes |
|---|---|---|
| a `.rs` file | 800 | 256 KiB |
| anything else | 200 | 64 KiB |

### A file's body

A body carries line numbers only where a change can start or end. `splice` locates a change by line,
so a session with `splice` numbers a file whose suffix has a sorter at each key line its table
stamps: a declaration, a
run of `use` statements, a hat of attributes and comments (its first and its last line, however many
it stacks; the file's own head counts even when a blank line follows it), the closing brace of any
numbered block, and — inside a function longer than fifteen lines — every block head: `if`, `else`,
`match`, `while`, `for`, `loop`, `unsafe` and `async`, whether it stands on its own, follows a `}`,
or opens behind `let x =`, an assignment, `return` or `break`. A `let … else` counts, and so does a
macro call or a closure whose `{` ends the line. No head needs its `{` on that line: a folded
condition puts it three lines down. Every other line stays unnumbered, so every numbered block has a
`(start, end)` a call can name without counting. A file no sorter takes is one whole piece per line:
every line carries a number.

`edit` locates a change by text, so a session with `edit` and without `splice` gets no numbers at
all: the body is the file, ready to be copied into `old`.

The gutter is as wide as the file's own last line number plus one pipe, so a three-digit file writes
`123|` and an unnumbered line writes `   |`: the pipe keeps the body in one column and makes the
boundary between gutter and code visible. An empty line is a newline and nothing else. The gutter's
bytes are part of the body's budget.

A status line opens the body when the file continues: `[800 of 900 lines; next offset 801]`, or
`[256 KiB cap; 161 of 200 lines; next offset 162]` when bytes ran out first. A line longer than the
room left is cut at a character boundary and not counted, so the next read starts at that same line.
A file that ended, and a window that starts past its end, answer no status line at all.

A file whose head names a known image format (PNG, JPEG, GIF, WEBP; the bytes decide, never the
name) answers one card line and attaches its bytes as an image part. The gate is the session's
`vision` flag: without it the image answers an error naming the refusal, with it the file still
obeys the 4 MiB cap. A file the head cannot name stays a text read and fails as before when it is
not UTF-8.

### A directory's entries

A directory answers one level: `name  N lines` for a file, `name/  K files, F dirs` for a directory.
Inside `src/` each child directory is opened one level too, folding to counts at ten files. The
session root lists its own entries, then `src/` as a section of its own with `src/`'s children
beneath it. Build junk — `target`, `node_modules`, dot entries — is named like anything else and
never opened. A listing past 800 lines ends with `[+N entries]`.

#### When a file carries its stamp

A level is one directory's own files, and every level is judged alone. Only `.md` files take a part:

| kind | what it is |
|---|---|
| a document | upper case: no lower-case letter in the stem, at least one upper-case one |
| a kept document | a document whose stem the user listed in `config.server.agent.tools.read.persistentDocs` |
| anything else | every other file, and every directory |

A level whose documents are all newer than every other file beside it is vouched for: no file at
that level carries a stamp, and each document says what it is instead.

```text
  BUG?.md(latest)  1 lines
  code.rs  3 lines
  DESIGN.md(persistent)  2 lines
  README.md(latest)  2 lines
```

Any other level stamps every file at that level, and only a kept document keeps a tag.

```text
  2023-11-15 06:13:21  BUG?.md  1 lines
  2023-11-15 06:13:40  code.rs  3 lines
  2023-11-15 06:13:21  DESIGN.md(persistent)  2 lines
```

A kept document is never weighed and never vouched for: it cannot earn `latest`, it cannot keep its
level from being vouched for, and it is stamped like any other file when the level is not. A level
holding no document at all is vouched for, so an ordinary source directory answers as it always
did. A file whose moment cannot be read leaves its level unjudged, which stamps the level.

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

## 2. Design intent

**Effective output over complete output.** Every read spends the model's context, so an answer
carries what a change needs and nothing else: the block edges a `splice` call can name, the
neighbours a module is edited against, the newest documents a level already vouches for.

**The entry file is the hinge.** `mod.rs`, the dual-track `x.rs`, `lib.rs` are what a crate is read
through, so the graph is found from `src/` outward and never by asking a compiler.

**A directory answer is two levels deep.** One level says what is here; the second says where a
module's files are, which is the question the model asks next.

**A stamp only where a judgment is needed.** A level whose documents are newer than its code needs
no date to be trusted, and the tool says so instead of printing twenty timestamps the model must
compare itself.

## 3. Unresolved

1. `pub use x::*`, `include!`, `#[path]` and `cfg` are not followed on the way up.
2. A module whose directory has no entry file is read by file names alone, and the view says so.

## 4. Config fields

| key | use |
|---|---|
| `config.server.agent.tools.read.persistentDocs` | the document stems the user keeps current: upper-cased, `.md` understood, default `DESIGN` |
