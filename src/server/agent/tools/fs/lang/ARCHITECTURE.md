# lang — architecture

What is here, what each file holds, who talks to whom. Current state only.

## Files

| file | holds |
|---|---|
| `mod.rs` | the registry: the `Language` contract, the language list, the suffix lookup, and the questions a tool asks — the table, the piece's kind, the relations it indexes, the graph facts it supplies, the source flag, the formatter, the checker, the tests |
| `graph/` | the graph machine: units, edges, the upward walk, the printing parts and the default narration; it names no language |
| `table.rs` | `Stamp`, `Line`, `Table`: one file's lines, each line's stamp, the line it names, its other end, the language that wrote it, and the queries over them |
| `verdict.rs` | what a formatter, a checker and a test run answer: the formatted text, the text that did not parse, one unit's check, one diagnostic, the tests one unit ran |
| `guages/mod.rs` | declares the languages |
| `guages/rust/mod.rs` | registers Rust: its suffixes, table, kinds, relations, graph facts, source flag, formatter, checker and tests |
| `guages/rust/table.rs` | the Rust sorter: head words, three key-line passes, head templates, what a piece is |
| `guages/rust/scan.rs` | Rust's own scan: a cursor and a line buffer Rust's delimiters drive, then the brace pairs and their closing lines |
| `guages/rust/relations.rs` | Rust's relation index: `A::b` as a module or type member, a lone name's declarations and impl blocks, and how it reads `::` and names |
| `guages/rust/facts.rs` | Rust's graph facts: the unit a file belongs to, the units an entry declares, its exports, its edges with their forwarding resolved, and the crate root as the domain |
| `guages/rust/view.rs` | Rust's narration: exports, down, out, up, and where the scan stopped |
| `guages/rust/test.rs` | Rust's test support: the tests a line range declares, the toolchain's own list of their full paths, and one run of those alone |
| `guages/markdown.rs` | the Markdown sorter: headings, fences, lists, quotes, HTML blocks, front matter, and how far each heading's section reaches |

## What it talks to

- **`gate`**: the process machine a formatter, a checker or a test run puts its child through; the
  language supplies the command line and reads the answer.
- **Nothing else in this crate but `summary`** through its consumers: a sorter answers text, never
  I/O.
- **Who calls it**: `read` (line numbers, budget, `lang::hub`, `lang::tree`, `lang::files`), `splice` (receipt
  anchors, breadcrumbs, the formatter, the check), `edit`, `fault` (the check and the tests), `change`,
  `skeleton`, `grep` (its scan, its outline, its three sections), `write` (the formatter, the check).
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
  |lang::relations(path)
  |  → owner(path)?.relations   a language that indexes none takes no part
  |  → rust::relations::scan    read the query: `A::b` a member, a lone name its declarations

  |lang::format(path)
  → owner(path)?.check       a language with no checker takes no part
  → fault::gather            one run per unit, with the files this batch wrote into it
  → rust::check::run         cargo check → its JSON → verdict::Check

lang::tests(path)
  → owner(path)?.test        a language with no tests takes no part
  → fault::gather_tests      the names each write's line range covers, one unit at a time
  → rust::test::find         the attributes above a `fn`, and the range they hold
  → rust::test::run          cargo test --list → the full paths → --exact → verdict::Verdict
```

## Must stay in step

1. `Table::span`, `Table::reach` and every consumer's reading of `Stamp`: an opening line's `mate`
   is its closing line, and a closing line's `mate` is the line it closes. `span` keys that closing
   line, `reach` leaves it as it was.
2. `Line::label` and `Line::exact`: a label is the head's template with the names cut out
   (`impl a for b`), an exact is the head's own text (`impl Display for Env`). A tool that judges
   what a piece is reads `lang::shape`; a tool that asks where a name is declared reads the
   language's own relation index (`lang::relations`). Neither cuts a label itself.

3. `Language`'s fields and `LANGUAGES`: a new language is one file under `guages/` and one entry in
   that list, and nothing else in this crate learns its name or its suffixes. `relations` is the
   language's own affair: `None` for one that names nothing, and every `::`/`.` it reads stays here.
4. A `Checker`'s `shown` and `fault`'s placement: a diagnostic reaches the write it belongs to by
   the name `shown` gives that file, and by the line the checker reports, so the two must agree.
5. `Table::plain` and `read`: a file with no sorter numbers every line, so a new sorter narrows
   what a tool shows and never widens it.
6. `Tests::find` and a piece's span: a test's reach — its first attribute to its closing line — is
   derived for the test alone and never written back into the table, so no other reader sees it.
