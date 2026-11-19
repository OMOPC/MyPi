# rust

Rust's own declarations, registered in `mod.rs`: its suffixes, line table, graph facts, formatter,
checker, tests, and relation index.

## Relations

`relations.rs` answers `A::b` and a lone name. It prints three sections, in this order:

- `[type] A::b` — a type member, or why no crate-defined `A` declares `b`;
- `[module] A::b` — a module member, or why no module `A` holds `b`;
- `[x-ray] A` — the declarations of `A` and the `impl` blocks that carry its members.

Two limits belong to this file alone, because only Rust knows what an `impl` block is:

| constant | value | what it caps |
|---|---|---|
| `MEMBERS` | 12 | members one block lists |
| `IMPLS` | 8 | `impl` blocks one type's x-ray lists |

The tool that asks for these rows never sees the limits: `Relations::scan` takes only the sources,
the root and the query, so a new language needs no change outside its own file.