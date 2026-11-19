# grep — architecture

## Now

- `tool.rs` — the tool's face: the parameter table (`query`, `regex`, `scope`, `lavish`), `run`
  (one walk, one render), and the one-line history summary.
- `scan.rs` — the walk: the zone (the nearest `Cargo.toml` above the cwd and its `src/`), the roots
  `scope` names (one miss line per unusable entry), the matcher (text or pattern), the parallel
  collection of every file `lang::files` names and `loader::text` reads, and the item grouping
  that turns a file's hit lines into the pieces the answer draws.
- `render.rs` — the answer: two header lines (counts, mode, tier, then the scope inventory), the
  language's relation block (`[type]` / `[module]` / `[x-ray]`), the tier judgment over the item
  count, the three densities (whole items, outlines, the census in its 500-row room), the footer
  (`DOWNSHIFTED`, `[+N more files]`, `[scope]`), and the stamps into `env.seen`. Each drawn line
  comes from `fs/skeleton`'s gutter, so every tool numbers a line the same way.

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
