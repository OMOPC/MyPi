# memo — architecture

## Files

| file | holds |
|---|---|
| `mod.rs` | declares the memo family |
| `context/{mod,tool}.rs` | `context`: `query` through `Source::history`, `show` through `Source::block`, long listings to `Export::dir` |
| `memo/{mod,tool}.rs` | the `memo` tool: writes as effects, reads through `MemoRead` |
| `todo/{mod,tool}.rs` | the `todo` tool: reads the anchor, replays, applies, writes the memo |
| `todo/replay.rs` | the pure replay: a call sequence → the list, anchored on the newest reset |
| `todo/ops.rs` | the three todo operations (`set`, `tick`, `view`) and their batch apply |
| `todo/state.rs` | `TodoItem`, `counts` |

## Relations

- `memo` depends on `contract::view` for `MemoRead` and the `Memo` effect; the loop lands the
  effect in the same commit as the result block.
- `todo` depends on the anchor the session records: a call sequence is replayed from the newest
  reset, so the tool never stores the list itself.
- `context` depends on `Source::history` and `Source::block`, and on `Export::dir()` for an answer
  past ten rows or three blocks.
- All three are consumed by the engine and the registry like every tool.
- Kept in step with `contract::output::MemoOp`: the effect a write names and the operation the
  loop applies are one enum.

## Must stay in step

1. `todo/replay.rs` and `todo/ops.rs`: the replay produces the list the operations apply to; a new
   operation starts in `ops` and is replayed there.
2. `memo`'s write effect and `contract::output::MemoOp`: the tool names the operation, the loop
   applies it; neither side may carry a second vocabulary.
3. `context`'s export name and `Export::dir()`: the name carries the session and the parameters, so
   one session overwrites its own file and no two sessions collide.