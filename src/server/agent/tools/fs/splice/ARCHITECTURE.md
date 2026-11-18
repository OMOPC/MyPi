# splice — architecture

What is here, what each file holds, who talks to whom. Current state only.

## Files

| file | holds |
|---|---|
| `mod.rs` | module declarations, and the two names the rest of the crate takes: `TOOL`, and `mass_text` / `parse_range` for `final` |
| `tool.rs` | the tool's face: the parameter table, the batch-aware `run` (one merged write per file, one answer per call, the audit's later half), the merged snapshot `compute`, and the flattening of every call's items into one list of ranges |
| `edit.rs` | one range: its snapshot coordinates, its block, the macros it fills and the ranges it cuts, the anchor and overlap rules, the reverse-order splice, where its lines land without a formatter, and the whole-line write `final` reuses |
| `fmt.rs` | the language's formatter through the gate: the merged text with a marker comment around every block, the markers' lines as each range's landing, the text without them, and the per-block trials that only name what the parser blames |
| `receipt.rs` | the answer: the verdict, then one row per range with the lines it took and the block in place, the reasons under them, and the segment memo a front end draws |

## What it talks to

- **`contract::env::Env`**: `workspace` (the cwd and the per-path write lock), `seen` (a file's
  licence: its line count and content hash, granted by `read` or `grep`), `batch` / `current` /
  `handed` (the calls of this batch, so calls on one file become one write), `stop`, `shells`.
- **`contract::view`**: `CallRecord`, for the one-line history summary.
- **`fs::lang`**: the line table the answer numbers by, the formatter a suffix names, and the
  checker a written file hands the batch's audit.
- **`fs::fault`**: `Shell` and `Slot`, the handover by which the compiler's faults reach the
  answer of the call whose lines they hit.
- **`fs::workspace`**: `write_atomic` and the per-path write lock.
- **`fs::change`**: the block's lines, and the receipt `final` reuses.
- **`platform::stop`, `platform::proc`**: the turn's stop signal, and the process group a
  formatter is killed with.
- **`spec`**: `Param`, `Kind::Axis`, `Args`, `Item`.
- **`summary`**: `flat`, the whitespace folding an anchor and a line are both compared by.
- **Who calls it**: `registry` holds `TOOL`; `fs/read` asks `TOOL.name` whether a Rust body carries
  line numbers at all; `subagent/final` reuses the whole-line write for a child's draft.

## Inside

```text
splice::TOOL
  → plans                  every call of this batch that names the same file, in arrival order
  → workspace::with_write  the one lock; the licence is compared before anything is parsed
  → compute
      → read_to_string     the snapshot every range is measured against
      → License            line count and hash, against the one `read` granted
      → flatten            each call's items → one list of `Edit`s, plus each call's slice
      → edit::settle_overlaps   anchors first, then a call's own overlaps (fatal), then cross-call ones
      → fmt::settle        the formatter's pass and each range's landing, or the plain splice
      → write_atomic       only when the text changed
  → Built::answers        one answer per call: its own rows, its own verdict, the audit's slots
  → env.shells            the handover the batch's check fills later
```

## Must stay in step

1. `edit::Edit`'s two anchor words and `drift` / `miss_at`: the rule that a bare number names a
   blank line only lives there, and `tool.rs` and the schema's `about` strings state it. Change one
   and the model is told something the tool does not do.
2. `Kind::Axis` and both fields of an edit item: the schema renders it as an integer or a string, so
   the parser and the referee are two halves of one rule.
3. `fmt::settle`'s landings and `receipt::row`: the answer names each block by the formatter's
   coordinates, so a changed walk-back changes both.
4. `Built` and `tool.rs`'s audit block: the checked file, its landing places and the per-call slots
   are built together, and `fs/fault` reads them as they are.
5. `mass_text` / `parse_range` and `subagent/final`: they are the only other consumer of the
   whole-line write, and they carry no anchor words at all.
6. `TOOL.name` and `fs/read`'s `Numbers`: a Rust body carries line numbers only while `splice` is
   in the session's list.
