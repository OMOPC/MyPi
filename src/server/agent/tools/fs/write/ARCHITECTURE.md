# write — architecture

## Files

| file | holds |
|---|---|
| `mod.rs` | module declarations and the tool export |
| `tool.rs` | the `write` tool: `path`, `content`, the formatter pass, the write whatever the formatter said, and the compiler's report unless the text does not parse |

## Relations

- Depends on `fs/lang` for the formatter and checker the file's language answers.
- Depends on `fs/gate` for the formatter and checker processes, their deadlines, and the group a
  stop kills.
- Depends on `workspace` for the licensed zones, the per-path write lock and `write_atomic`.
- Hands its written shell to `fs/fault.rs`'s audit like every written call; the diagnostics land
  under the answer as a source seam, once.
- Consumed by the engine and the registry like every tool.
- Kept in step with `fs/splice`: both read a file's formatter outcome and the language's own
  `Outcome` and `Check`; the answer borrows them there and never rebuilds them here.

## Must stay in step

1. The formatter outcome and the language's `Outcome` are one answer; a change to what a language
   reports reaches both `write` and `splice`.
2. The audit's unit and the written file: the file is checked inside the crate that holds it, and
   one batch gives one check per crate.
3. The write lock and the atomic rename are `workspace`'s; `write` never opens the target directly.