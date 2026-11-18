# grep — architecture

## Now

- `tool.rs` — the tool's face: the parameter table (`query`, `regex`, `scope`, `lavish`), `run`
  (one walk, one render), and the one-line history summary.
- `scan.rs` — the walk: the zone (the nearest `Cargo.toml` above the cwd and its `src/`), the roots
  `scope` names (one miss line per unusable entry), the matcher (text or pattern), the parallel
  collection of every readable file, and the item grouping that turns a file's hit lines into the
  pieces the answer draws; a file over 4 MiB is out.
- `render.rs` — the answer: the header (hits, items, files, mode, tier), the folded reminder, the
  language's relation block (`[type]` / `[module]` / `[x-ray]`), the tier judgment over the item
  count, the three densities (whole items, skeletons, the census in its 500-line room), the
  `[scope]` lines, and the stamps into `env.seen`.

## Relations

- Depends on `fs/lang` for every structural fact: the line table (`innermost`, `around`, `end`,
  `key`) and the language's own relation index. Labels are opaque strings here; no language is
  special-cased.
- Depends on `contract::env` for the caps (`GrepLimits`) and the stamp (`Seen`, `License`); the loop
  fills both from config when it books a session.
- Depends on `fs::workspace` only for a lenient canonicalize of scope paths.
- Consumed by the engine and the registry like every tool; nothing reaches into grep's internals.
- Kept in step with `fs/lang`: the sorter answers the table, grep answers the showing — display
  caps, folding and prose never leak into `lang`.
