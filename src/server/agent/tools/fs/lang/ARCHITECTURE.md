# lang — architecture

What is here, what each file holds, who talks to whom. Current state only.

## Files

| file | holds |
|---|---|
| `mod.rs` | the sorter a path's suffix names, or a table where every line stands alone |
| `table.rs` | `Stamp`, `Line`, `Table`: one file's lines, each line's stamp, the line it names, its other end, and the queries over them |
| `blank.rs` | `Blank` and the shared walk: a cursor and a line buffer a language's delimiters drive, then the brace pairs and their closing lines |
| `rust.rs` | the Rust sorter: its suffix, head words, three key-line passes, head templates, what a piece binds, and which characters open a comment or a literal |

## What it talks to

- **Nothing in this crate but `summary`** through its consumers: a sorter answers text, never I/O.
- **Who calls it**: `read` (line numbers, `hub/syntax`), `splice` (receipt anchors, breadcrumbs),
  `edit`, `fault`, `change`, `skeleton`, `grep` (its scan, its outline, its three sections).
- **Knows nothing about**: the workspace, the formatter, the compiler, any tool's caps or prose.

## Inside

```text
lang::table(path, text)
  → rust::SUFFIXES          does this suffix have a sorter
  → rust::table             blank, pair the braces, then three passes
      → blank::strip        comments and literals cut out, line count kept
      → blank::ends         each opening line's closing line
      → pieces              every declaration with its head template and its own header
      → hats                comment and attribute blocks: first and last line
      → control             a long function's own block heads
  → Table::plain            a suffix with no sorter: every line its own piece
```

## Must stay in step

1. `Table::span` and every consumer's reading of `Stamp`: an opening line's `mate` is its closing
   line, and a closing line's `mate` is the line it closes.
2. `Line::label` and `Line::exact`: a label is the head's template with the names cut out
   (`impl a for b`), an exact is the head's own text (`impl Display for Env`). A tool that judges
   what a piece is reads `rust::shape`; a tool that asks whether it binds a word reads
   `rust::binds`. Neither cuts a label itself.
3. `blank::Blank` and `rust::token`: a sorter's delimiters drive the shared walk, so a new language
   adds a token function and nothing else.
4. `Table::plain` and `read`: a file with no sorter numbers every line, so a new sorter narrows
   what a tool shows and never widens it.