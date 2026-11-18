# grep

The `grep` tool: one query over the crate's own `src/`, answered as text.

## Design / Schema

- The crate is the nearest `Cargo.toml` above the session's directory; only files a sorter claims
  (`.rs`, `.md`) under its `src/` are read. No ignore rules and no external binary, so the answer is
  the same on every platform, and `target/` is out of reach by construction. Anything outside
  `src/` — `Cargo.toml`, `build.rs`, the root documents — is one `read` away and never searched.
  Files are read in parallel; a file over 4 MiB is skipped.
- Parameters, in index order:
  - `query` — always; the literal text to find, or the pattern when `regex` is on. A pattern that
    does not compile is refused with the regex engine's own position and a reminder that `regex` is
    off by default.
  - `regex` — default false; parse `query` as a pattern instead of text.
  - `scope` — paths named from the crate root and starting with `src/`: a directory, or a `.rs` or
    `.md` file; omitted, the whole `src/`. An entry that is missing, names no readable file, or
    stands outside `src/` is skipped while the rest still run, and the answer ends with one
    `[scope]` line per such entry naming the reason.
  - `lavish` — default false; the whole answer is drawn one tier denser: tier 3 as 2, tier 2 as 1.
- Structure comes from the language sorter (`fs/lang`), the same table `read`, `splice` and `edit`
  read: comments and literals blanked, braces paired, head words recognized. Each line carries a
  stamp, the shape it reads by, and the line it names. A Markdown document is read by its own
  grammar: headings, fences, lists, quotes. Nothing here knows a language's own words.
- A hit is a line. Every matching line counts once, whatever it holds — comment, literal, prose —
  and several matches inside one line are still one hit.
- An item is the drawn unit. A hit takes its innermost piece; a span that lands inside an item
  joins it, and a later span that swallows it widens it to that outer piece. So an item is the
  smallest piece holding its run of nested hits, and pieces that merely stand side by side never
  merge: two `impl`s each holding a hit are two items. A hit no piece holds is its own one-line
  item. Every hit belongs to exactly one item.
- The answer opens with one header line — hits, items, files, mode, tier — then the three sections
  the language's own relation index fills (`[type]`, `[module]`, `[x-ray]` for Rust; a language
  that names none skips them), then the items.
- Tiers are judged by the answer's item count, against `deepMax` and `foldMax`:
  - Tier 1 — up to `deepMax` items: every line of every item, whole, whatever their length. Only
    key lines and hit lines carry a number; each block opens with `path:line` and an `in:`
    breadcrumb of the pieces above it.
  - Tier 2 — up to `foldMax` items: the skeleton. Each item's ancestors' head lines, once per file,
    then only the key lines and hit lines inside the item; the item's own head row ends with
    `..close`, so both ends of a splice are on the page. A piece that holds no hit is not drawn at
    all — no fold row, no count, nothing.
  - Tier 3 — past `foldMax`: one row per file that holds an item, widest first, within the
    answer's own 500-line room; what the room cannot hold is one `[+N more files]` line.
  - A tier 2 or tier 3 answer carries `[folded: …]`: go `lavish`, or narrow the scope — whichever
    moves are still open.
- `lavish` lifts the whole answer one tier and never mixes two densities; tier 1 is already the
  densest and stays. Under `lavish` the hint names the one move left, narrowing the scope.
- Every file whose lines an answer showed is stamped in `env.seen` — line count and content hash,
  the stamp `read` writes — so `splice` works on it without a second `read`.
- The front door does not measure a grep answer. Tiers 1 and 2 draw the whole truth by design;
  tier 3 carries its own 500-line room, and that is the only cap the tool knows.
- What it does not read: anything a macro generates, the `impl`s a derive writes, `include!`,
  `#[path]`, and a type renamed by `use … as`. A hit inside such code is still a hit; only its
  breadcrumb is coarser.

## Design intent

- **One query, the whole crate.** A search that names no scope answers the whole `src/` at once;
  narrowing is the model's choice, never a prerequisite.
- **The sorter answers the structure, the tool answers the showing.** All caps, folding and prose
  live here; `fs/lang` never learns how grep draws.
- **A number is a line a splice may name.** Only key lines and hit lines are numbered, so a number
  fed back to `splice` or `edit` always lands on a line those tools name.
- **The item is the unit of everything.** Counting, drawing and folding all speak in items: one hit
  line and a whole function are each one item, and a file's item count is what the tiers weigh.
- **Density is the only dial.** Tier 1 and 2 never cut for length; tier 3 is counts alone, and its
  500-line room is the one budget in the tool.
- **A prompt that names the next move.** `[folded: …]` is printed only while a move is still open:
  it offers `lavish` until the answer is already lavish, and narrowing otherwise.

## Unresolved

None.

## Config fields

- `config.server.agent.tools.grep.deepMax` — 6: items a tier 1 answer hands over whole.
- `config.server.agent.tools.grep.foldMax` — 25: items a tier 2 answer sketches before the rest
  folds to per-file counts.
- `config.server.agent.tools.grep.xrayImpls` — 8: `impl` blocks one x-ray may list.
- `config.server.agent.tools.grep.xrayItems` — 12: members one block may list.
