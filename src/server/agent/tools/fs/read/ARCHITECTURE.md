# read — architecture

What is here, what each file holds, who talks to whom. Current state only.

## Files

| file | holds |
|---|---|
| `mod.rs` | module declarations and the one re-export |
| `tool.rs` | the `read` tool: the path array, the per-kind budgets and the per-item `limit` under them, one path's answer, and the module view that follows a source body |
| `dir.rs` | directory listings: one level per directory, each level's stamp verdict, entry counts, and the per-level fold that keeps a wide directory to ten lines, junk named but never opened |
| `items.rs` | the item mode: the pieces whose own header equals the one the model named, under one file or a whole directory |
| `sniff.rs` | an image's format from its first bytes |

## What it talks to

- **`contract::env::Env`**: `workspace` (the session's cwd), `seen` (the module cards shown and the
  stamp of every file read), `tools` (whether `splice` is allowed), `vision`, `persistent_docs`.
- **`contract::view`**: `CallRecord`, for the one-line history summary.
- **`workspace::canonicalize_lenient`**: the one path form every comparison uses.
- **`lang`**: the line table, which says what a `pub` line opens and whether it is an item; the
  graph facts a language supplies; and `lang::tree`, which says whether a directory belongs to a
  source tree.
- **`lang::graph`**: `unit`, `build` and `render` — one file's neighbourhood, printed by its
  language or by the machine's default.
- **`platform::time`**: the local stamp a listing prints.
- **`splice::TOOL`**: the gate that decides whether a Rust body carries line numbers.
- **Who calls it**: `registry` holds `TOOL`; the front door runs it like any other tool.

## Inside

```text
read::TOOL
  → dir::listing          a directory: its counts for the header, then one level's entries, each
                          level's verdict, then the children
  → items::answer         a named header: every piece under the path whose header equals it
  → block                 one file: an image part with no text, or a body
      → Numbers           the sorter's key lines under splice, every line under a file it does not
                          take, none under edit
      → assemble          the window and its gutter; the status line is the footer
      → lang::hub         the graph facts this file's language supplies
          → graph::unit   the unit the file sits in
          → graph::build  the unit's neighbourhood
          → graph::render its rows, by the language's narration or the default
```

## Must stay in step

1. `read`'s caps and `engine`: the front door skips `measure` for `read` because the tool cuts its
   own files; remove either half and a single-line file reaches the context whole.
2. `dir.rs`'s verdict and `Env.persistent_docs`: a configured stem is compared with a file's stem
   exactly, so a name the config upper-cases still matches nothing on disk written lower case.
