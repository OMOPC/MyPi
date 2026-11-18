# grep — architecture

## Now

- `tool.rs` — the tool's face: the parameter table (`query`, `regex`, `scope`, `lavish`), `run`
  (one walk, one render), and the one-line history summary.
- `scan.rs` — the walk: the zone (the nearest `Cargo.toml` above the cwd and its `src/`), the roots
  `scope` names (one miss line per unusable entry), the matcher (text or pattern), and the parallel
  collection of every readable file; a file over 4 MiB is out.
- `render.rs` — the answer: the header, the folded reminder, the language's relation block
  (`[type]` / `[module]` / `[x-ray]`), the tier judgment over `deepMax` and `foldMax` with `lavish`
  lifting the drawn tier by one, the three tier renderers, the budget cut, the `[scope]` lines, and
  the stamps into `env.seen`.
- `outline.rs` — tier 2's drawing: per file, the trees its hits hang in. Every hit's chain of pieces
  opens level by level with real lines and closes innermost first; the pieces beside a chain fold to
  one row per run of one label, `start::end label …`.

## Relations

- Depends on `fs/lang` for every structural fact: the line table (`around`, `inner`, `end`) and the
  language's own relation index. Labels are opaque strings here; no language is special-cased.
- Depends on `contract::env` for the caps (`GrepLimits`) and the stamp (`Seen`, `License`); the loop
  fills both from config when it books a session.
- Depends on `fs::workspace` only for a lenient canonicalize of scope paths.
- Consumed by the engine and the registry like every tool; nothing reaches into grep's internals.
- Kept in step with `fs/lang`: the sorter answers the table, grep answers the showing — display
  caps, folding and prose never leak into `lang`.
