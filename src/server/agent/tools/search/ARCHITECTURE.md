# search — architecture

## Files

| file | holds |
|---|---|
| `mod.rs` | the tool entry: the same `WebArgs`, one query against one engine |

## Relations

- Depends on `crate::web` for `WebArgs` and `run`; the engines and their parsing stay there.
- Consumed by the engine and the registry like every tool.
- Kept in step with `src/web/README.md`: the provider names and the answer shape are the domain's.

## Must stay in step

1. The `provider` values and `src/web`'s provider set: a new engine starts in both.
2. `pages` and the domain's page walk: the tool passes the count; the domain decides when a page
   adds nothing new.