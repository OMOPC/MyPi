# edit — architecture

## Files

| file | holds |
|---|---|
| `mod.rs` | module declarations and the tool export |
| `tool.rs` | the `edit` tool: `path`, `old`, `new`, `unfold`; the licensed read, the unique-run match, `exact_text`, the one-line receipt, the excerpt memo, and the audit handover |

## Relations

- Depends on `workspace` for the licensed zones, the per-path write lock and `write_atomic`; the
  read, the match and the write happen inside one lock, and the read must already hold `env.seen`'s
  licence.
- Depends on `fs/lang` for the line table's pieces, so the memo's item and breadcrumb come from
  the same structure the other file tools read.
- Depends on `fs/change` for the excerpt around the change, its breadcrumb and the receipt's
  line arithmetic.
- Hands its written shell to `fs/fault.rs`'s audit like every written call; the diagnostics land
  under the answer that wrote the lines.
- Consumed by the engine and the registry like every tool.
- Kept in step with `fs/splice`: either name may be in a session's list, never both; the mutual
  exclusion is settled in `config/profile/resolve.rs` and recorded in `../../README.md`.

## Must stay in step

1. The item a memo draws and the piece `fs/lang` reports are the same piece; a change to what
   counts as a piece reaches both.
2. The receipt's line arithmetic and the write it describes come from the same change; a change to
   the landing rule changes both together.
3. `fullItembudget` and the memo's window: the window is the fallback whenever the item is longer
   than that key, so the default and the read must move together.