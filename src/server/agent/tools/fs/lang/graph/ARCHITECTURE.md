# graph — architecture

What is here, what each file holds, who talks to whom. Current state only.

## Files

| file | holds |
|---|---|
| `mod.rs` | the types (`Unit`, `Decl`, `Child`, `Edge`, `Around`, `Boundary`, `RawEdge`, `Domain`, `Size`), and the two doors: `unit` and `build`/`render` |
| `loader.rs` | the one read-only disk door: `Loader` with a per-call cache, plus filtered and unfiltered directory entries |
| `index.rs` | turning facts into a neighbourhood: unit lookup, child expansion, size, incoming/outgoing, declared-by |
| `walk.rs` | the upward search: `Levels` and `Tree`, the deadline, whole-level drops, the boundary |
| `parts.rs` | the printing parts: relative paths, plurals, clipping, headings, node/edge/boundary lines, grouped importers, the view budget |
| `view.rs` | the default narration, for a language with no `view` of its own |

## What it talks to

- **`lang::Hub`**: the facts, as function pointers. This layer never names a language.
- **`lang::suffixes(hub)`**: the suffixes a walk may read, so foreign files are never parsed.
- **`lang::Row`**: the row shape every narration answers.
- **`read`**: the only consumer today; it decides when to build, keeps the one-primer-per-unit
  memory, stamps the file it read, and applies the view budget.
- **Who calls it**: `read::tool` through `lang::hub`.

## Inside

```text
graph::unit(facts, file)          the unit without building anything
graph::build(facts, loader, file) the neighbourhood
  → index::build
      → facts.unit_of / domain
      → facts.territory          children, with line and tags
      → facts.interface          per unit text
      → facts.edges              per unit text, landing files already resolved
      → walk::scan               incoming edges, plus the boundary
graph::render(facts, around, loader, batch)
  → facts.view, or view::default_view
```

## Must stay in step

1. `Boundary` fields and a language's scan line: `stop`, `complete` and `no_above` are the three
   facts a view needs to say why a walk ended.
2. `Edge.to` and `Edge.to_file`: incoming matching uses the unit, child dependencies use the file.
   Collapsing one into the other loses either the unit count or the sibling names.
3. `walk`'s suffix filter and `lang::suffixes`: a language that answers for a pointer but is not in
   `LANGUAGES` would walk every file.
