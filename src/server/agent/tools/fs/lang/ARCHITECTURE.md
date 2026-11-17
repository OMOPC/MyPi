# lang — architecture

What is here, what each file holds, who talks to whom. Current state only.

## Files

| file | holds |
|---|---|
| `mod.rs` | the registry: the `Language` contract, the language list, the suffix lookup, and the questions a tool asks — the table, the piece's kind, the name it binds, the source flag, the formatter, the checker |
| `table.rs` | `Stamp`, `Line`, `Table`: one file's lines, each line's stamp, the line it names, its other end, the language that wrote it, and the queries over them |
| `verdict.rs` | what a formatter and a checker answer: the formatted text, the text that did not parse, one unit's check, one diagnostic |
| `guages/mod.rs` | declares the languages |
| `guages/rust/mod.rs` | registers Rust: its suffixes, table, kinds, names, source flag, formatter and checker |
| `guages/rust/table.rs` | the Rust sorter: head words, three key-line passes, head templates, what a piece binds |
| `guages/rust/scan.rs` | Rust's own scan: a cursor and a line buffer Rust's delimiters drive, then the brace pairs and their closing lines |
| `guages/rust/format.rs` | the Rust formatter: rustfmt under a deadline, the edition its manifest names, and the verdicts it remembers |
| `guages/rust/check.rs` | the Rust checker: the crate a file belongs to, its name, how it names its files, and `cargo check` with its JSON decoded into diagnostics |
| `guages/markdown.rs` | the Markdown sorter: headings, fences, lists, quotes, HTML blocks, front matter, and how far each heading's section reaches |

## What it talks to

- **`gate`**: the process machine a formatter or a checker runs its child through; the language
  supplies the command line and reads the answer.
- **Nothing else in this crate but `summary`** through its consumers: a sorter answers text, never
  I/O.
- **Who calls it**: `read` (line numbers, budget, `hub/syntax`), `splice` (receipt anchors,
  breadcrumbs, the formatter, the check), `edit`, `fault` (the check), `change`, `skeleton`, `grep`
  (its scan, its outline, its three sections), `write` (the formatter, the check).
- **Knows nothing about**: the workspace, the compiler's own vocabulary, any tool's caps or prose.

## Inside

```text
lang::table(path, text)
  → owner(path)              does this suffix meet a language
  → rust::table              blank the comments and literals, pair the braces, then three passes
      → rust::scan::strip    comments and literals cut out, line count kept
      → rust::scan::ends     each opening line's closing line
      → pieces, hats, control
  → markdown::table          one pass: front matter, fences, headings, quotes, HTML blocks, lists;
                             then how far every heading's section reaches
  → Table::plain             a suffix no language names: every line its own piece

lang::format(path)
  → owner(path)?.format      a language with no formatter answers nothing, and the text is kept
  → rust::format             rustfmt under a deadline → verdict::Outcome

lang::checker(path)
  → owner(path)?.check       a language with no checker takes no part
  → fault::gather            one run per unit, with the files this batch wrote into it
  → rust::check::run         cargo check → its JSON → verdict::Check
```

## Must stay in step

1. `Table::span`, `Table::reach` and every consumer's reading of `Stamp`: an opening line's `mate`
   is its closing line, and a closing line's `mate` is the line it closes. `span` keys that closing
   line, `reach` leaves it as it was.
2. `Line::label` and `Line::exact`: a label is the head's template with the names cut out
   (`impl a for b`), an exact is the head's own text (`impl Display for Env`). A tool that judges
   what a piece is reads `lang::shape`; a tool that asks whether it binds a word reads
   `lang::binds`. Neither cuts a label itself.
3. `Language`'s fields and `LANGUAGES`: a new language is one file under `guages/` and one entry in
   that list, and nothing else in this crate learns its name or its suffixes.
4. A `Checker`'s `shown` and `fault`'s placement: a diagnostic reaches the write it belongs to by
   the name `shown` gives that file, and by the line the checker reports, so the two must agree.
5. `Table::plain` and `read`: a file with no sorter numbers every line, so a new sorter narrows
   what a tool shows and never widens it.
