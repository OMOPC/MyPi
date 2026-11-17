# read — architecture

What is here, what each file holds, who talks to whom. Current state only.

## Files

| file | holds |
|---|---|
| `mod.rs` | module declarations and the one re-export |
| `tool.rs` | the `read` tool: the path array, the per-kind budgets and the per-item `limit` under them, one path's answer, and the module view that follows a Rust body |
| `dir.rs` | directory listings: one level per directory, each level's stamp verdict, entry counts, junk named but never opened |
| `sniff.rs` | an image's format from its first bytes |
| `hub/mod.rs` | declares the graph's files and re-exports its outward names |
| `hub/locate.rs` | the module a file belongs to: its entry file, its crate, its chain, and a path relative to the crate |
| `hub/entries.rs` | one directory's entries, in the order a listing and a walk both read them |
| `hub/syntax.rs` | Rust read by line: `mod` declarations, `pub` items, `pub use`, and the use trees under them, each judged through the language sorter |
| `hub/edges.rs` | the edges: what one file reaches inside its own directory, and the names a `pub use` forwards |
| `hub/up.rs` | the upward scan: who imports this module, one whole level at a time, under a deadline |
| `hub/view.rs` | the view's text: exports, the modules the entry declares, and the importers above it |

## What it talks to

- **`contract::env::Env`**: `workspace` (the session's cwd), `seen` (the module cards shown and the
  stamp of every file read), `tools` (whether `splice` is allowed), `vision`, `persistent_docs`.
- **`contract::view`**: `CallRecord`, for the one-line history summary.
- **`workspace::canonicalize_lenient`**: the one path form every comparison uses.
- **`lang`**: the line table, which says what a `pub` line opens and whether it is an item.
- **`platform::time`**: the local stamp a listing prints.
- **`splice::TOOL`**: the gate that decides whether a Rust body carries line numbers.
- **Who calls it**: `registry` holds `TOOL`; the front door runs it like any other tool.

## Inside

```text
read::TOOL
  → dir::listing          a directory: one level's entries, each level's verdict, then the children
  → block                 one file: an image card, or a body
      → Numbers           the sorter's key lines under splice, every line under a file it does not
                          take, none under edit
      → assemble          the window, its gutter, its status line
      → hub::locate       the module the file sits in
          → hub::view     exports, down, up, scan
```

## Must stay in step

1. `read`'s caps and `engine`: the front door skips `measure` for `read` because the tool cuts its
   own files; remove either half and a single-line file reaches the context whole.
2. `dir.rs`'s verdict and `Env.persistent_docs`: a configured stem is compared with a file's stem
   exactly, so a name the config upper-cases still matches nothing on disk written lower case.
3. `hub::view`'s budget and `text::cut`: the view and the body bound themselves separately, so a
   change to one leaves the other alone.
