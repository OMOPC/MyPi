# lang — design

One language, one sorter; every file tool reads the same table.

## 1. Design

A sorter answers one line table for one file. Each line carries three things:

- a **stamp**: open, close, single, or plain. An opening line and its closing line pair up into
  "this piece runs from line N to line M".
- a **label**: the head's template, with the names cut out. `fn`, `pub struct`, `pub async fn`,
  `impl a for b`, `use`. Open text, not an enum, so a language may invent as many shapes as it
  has.
- an **exact**: the head's own text, its body's braces dropped. `impl Display for Env`,
  `pub async fn send()`, `use a::b`.

The two together mean a consumer never reparses a header. A tool that asks what a piece is reads
the label's head word; a tool that asks whether a piece binds a word reads the exact through the
sorter's own `binds`.

Suffixes point at sorters. A file whose suffix has none is a table where every line is its own
piece, so every line is a key line and every tool behaves as it did before a sorter existed.

## 2. Design intent

**One answer, many readers.** `read` numbers lines by it, `splice` anchors its receipt on it,
`edit` and `change` find the piece around a change, `fault` draws its seam, `grep` builds its
outline and its three sections. Each used to carry its own copy of the same parsing; a bug fixed in
one copy left the others wrong.

**Structure, not layout.** A sorter says what the pieces are and where they end. It never says how
they should look: clipping, indentation, folding and prose stay in the tool that shows them.

**One pass ends the ambiguity.** A piece ends where its braces close. Indentation is a formatting
habit, not syntax, and a second reading based on it disagrees with this one exactly often enough to
make both untrustworthy.

## 3. Not solved

- Markdown drafts have no sorter, so a breadcrumb over a subagent's draft is empty.
- Which lines a long function should key inside its body is decided by a word list in the Rust
  sorter; another language will have its own idea.