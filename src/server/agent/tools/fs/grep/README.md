# grep

The `grep` tool: one query over the crate's own `src/`, answered as text.

## Design / Schema

- The crate is the nearest `Cargo.toml` above the session's directory; only `.rs` and `.md` files
  under its `src/` are read. No ignore rules and no external binary, so the answer is the same on
  every platform, and `target/` is out of reach by construction. Anything outside `src/` —
  `Cargo.toml`, `build.rs`, the root documents — is one `read` away and never searched. Files are
  read in parallel; a file over 4 MiB is skipped.
- Parameters, in index order:
  - `query` — always; the literal text to find, or the pattern when `regex` is on. A pattern that
    does not compile is refused with the regex engine's own position and a reminder that `regex` is
    off by default.
  - `regex` — default false; parse `query` as a pattern instead of text.
  - `scope` — paths named from the crate root and starting with `src/`: a directory, or a `.rs` or
    `.md` file; omitted, the whole `src/`. An entry that is missing, names no readable file, or
    stands outside `src/` is skipped while the rest still run, and the answer ends with one
    `[scope]` line per such entry naming the reason.
  - `lavish` — default false; a folded answer draws one tier richer, and only one: the pool's front
    rises to the richer tier's density, the rest keeps its own. It never widens anything.
- Structure comes from the language sorter (`fs/lang`), the same table `read`, `splice` and `edit`
  read: comments and literals blanked, braces paired, head words recognized. Each line carries a
  stamp, the shape it reads by, and the line it names. A Markdown document is read by its own
  grammar: headings, fences, lists, quotes. Nothing here knows a language's own words; the folds
  name whatever label the sorter writes.
- The answer opens with one header line, then three sections the language's own relation index
  fills (`[type]`, `[module]`, `[x-ray]` for Rust; a language that names none skips them), then the
  hits.
- Hits are answered at the density their count allows, by `deepMax` and `foldMax`:
  - Tier 1 — up to `deepMax` hits: each hit comes with its innermost item whole, whatever the item's
    size, under an `in:` breadcrumb of every item around it. An excerpt line carries its number only
    when it is a hit or a key line of the sorter's own table, so a number means a line `splice` may
    start or end on. A hit no piece holds hands over its own line alone.
  - Tier 2 — up to `foldMax` hits: each file answers as the trees its hits hang in. For every hit
    its own chain of pieces opens level by level: the real opening lines with their numbers, the
    hit line at its place, the closings drawn as the file has them — every row in the file's own
    order. The pieces beside a chain fold to one row per run of one label, `start::end label …`,
    set where a sibling's text starts; a run breaks at a different label or at a chain child. A
    file whose hits hang under several top pieces draws one tree per piece, and a hit no piece
    holds is its own one-row tree.
  - Tier 3 — past `foldMax`: per-file hit counts only, widest first, within its own 500-row room —
    the one bar a grep answer knows, and a room only the census draws from. With `lavish`, the
    pool's front is sketched out of that same room first.
  - `lavish` on a tier 2 or tier 3 answer mixes two densities: counted tier 2 hands the first
    `deepMax` hits whole and sketches the rest; counted tier 3 sketches the first `foldMax` hits
    and counts the rest. Each mixing states its own account — `[whole: …]`, `[sketched: …]` — and
    every cap or room that bites names what it left out; nothing is dropped silently.
  - A tier 2 or tier 3 answer carries `[folded: …]`: go `lavish`, or narrow the scope — whichever
    moves are still open. `fileMax` caps one file's hits in one answer, and the queue it feeds is
    files in name order, lines top down.
  what it left undrawn.
- Every file whose lines an answer showed is stamped in `env.seen` — line count and content hash,
  the stamp `read` writes — so `splice` works on it without a second `read`.
- `grep` bounds its own text, so the front door does not measure it: what it answers is what the
  model reads, however long that is.
- What it does not read: anything a macro generates, the `impl`s a derive writes, `include!`,
  `#[path]`, and a type renamed by `use … as`. A hit inside such code is still a hit; only its
  breadcrumb is coarser.

## Design intent

- **One query, the whole crate.** A search that names no scope answers the whole `src/` at once;
  narrowing is the model's choice, never a prerequisite.
- **The sorter answers the structure, the tool answers the showing.** All caps, folding and prose
  live here; `fs/lang` never learns how grep draws.
- **A number means a spliceable line.** Tiers 1 and 2 only number lines the table keys, so a line
  number can be fed back to `splice` or `edit` unchanged.
- **Density over budget.** A grep answer is never cut for length; the census alone has its own
  500-row room, and the rest of the tiers draw the whole truth. A mixed answer states its account.
  it is off, and narrowing otherwise; it never asks for what is already done.

## Unresolved

None.

## Config fields

- `config.server.agent.tools.grep.deepMax` — 6: hits a tier 1 answer hands over whole; with
  `lavish`, the pool's front that rises to tier 1.
- `config.server.agent.tools.grep.foldMax` — 25: hits a tier 2 answer may sketch before the rest
  folds to per-file counts; with `lavish`, the pool's front that rises to tier 2.
- `config.server.agent.tools.grep.xrayImpls` — 8: `impl` blocks one x-ray may list.
- `config.server.agent.tools.grep.xrayItems` — 12: members one block may list.
