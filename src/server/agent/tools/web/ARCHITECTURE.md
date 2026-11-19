# web — architecture

## Files

| file | holds |
|---|---|
| `mod.rs` | the tool entry: arguments to `WebArgs`, one call to the domain |

## Relations

- Depends on `crate::web` for `WebArgs` and `run`; every browser fact stays there.
- Consumed by the engine and the registry like every tool.
- Kept in step with `src/web/README.md`: the domain's config keys and operations are the ones this
  entry names, and a rename on either side is a rename on both.

## Must stay in step

1. The `type` values and `src/web`'s `op` values: one enum, two names; a new operation starts in
   both.
2. The fields this entry reads and `WebArgs`: the tool's parameter table is the only place the
   model learns the names.