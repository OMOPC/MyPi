# grep — architecture

What is here, what each file holds, who talks to whom. Current state only.

## Files

| file | holds |
|---|---|
| `mod.rs` | module declarations and the tool export |
| `tool.rs` | the tool's face: its parameters, the one walk and the one-line history summary |
| `scan.rs` | the walk: the tree a language claims, the roots a call narrows, the matcher, the parallel file collection and the item grouping |
| `render.rs` | the answer: header lines, the relation block, the tier judgment, the three densities, the footer and the read stamps |

## Relations

- Depends on `fs/lang` for every structural fact: the line table and the language's own relation index. Labels are opaque strings here; no language is special-cased.
- Depends on the tool contract for the tier caps and the read stamp; the loop fills both from config when it books a session.
- Depends on `fs::workspace` only for a lenient canonicalize of scope paths.
- Consumed by the engine and the registry like every tool; nothing reaches into grep's internals.
- Kept in step with `fs/lang`: the sorter answers the table, grep answers the showing — display caps, folding and prose never leak into `lang`.