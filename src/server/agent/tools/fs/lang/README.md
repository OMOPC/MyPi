# lang — design

One language, one file; every file tool reads the same table and asks the same questions.

## 1. Design

A language answers one line table for one file, and declares what else it can do. Every sorted
language is one file under `guages/`.

**The table.** For every line: what that line is (an opening line, its closing line, one whole piece
on one line, or nothing), the line it pairs with, the label its head reads by, and the head's own
text. An opening line and its closing line pair into "this piece runs from line N to line M".

**The declarations.** A language says what a tool may ask it:

- the suffixes it answers to;
- whether its pieces can index relations, and how: `A::b`, a lone name, and the symbol it reads;
- whether it knows its units and edges (`Hub`): the unit a file belongs to, the units a unit
  declares, what a file exposes, where to look for importers, and how to narrate the result;
- whether its files are source, which decides how much of one a read shows;
- whether it has a formatter, and how to run it on a text;
- whether it has a checker, and how to find and run the unit a file belongs to.

A **stamp** is one of four: open, close, single, plain. A **label** is the head's template with the
names cut out (`fn`, `impl a for b`); an **exact** is the head's own text (`impl Display for Env`,
`## 设计意图`). A consumer never reparses a header.

A piece's last line may be a key line of its own — Rust's `}`, a fence's second line, a list's last
line — or ordinary prose, which carries no number: a heading's section ends at the line before the
next heading. A sorter says which by which of the table's two writing methods it calls.

Suffixes point at languages. A file whose suffix meets none is a table where every line is its own
piece, and no declaration is there to read.

## 2. Design intent

**One answer, many readers.** `read` numbers lines by it, `splice` anchors its receipt on it,
`edit` and `change` find the piece around a change, `fault` draws its seam, `grep` builds its
outline and its three sections. Each used to carry its own copy of the same parsing; a bug fixed in
one copy left the others wrong.

**One language, one file.** Every fact about a language — its structure, its formatter, its checker
— lives in that language's own file. A tool asks the declarations; it never names a language, a
suffix or a toolchain.

**The checker knows its own unit.** A build unit is whatever the language says it is: a crate, a
module, a package. The shared side groups the batch's writes by unit, runs the language's checker
once per unit after the last write, and hangs every diagnostic under the edit whose lines it hit. A
language with no checker takes no part.

**The process machine belongs to no language.** Starting a child, its deadline, its process group
and the turn's stop are `gate`'s; a language supplies the command line and reads what came back.

**Structure, not layout.** A sorter says what the pieces are and where they end. It never says how
they should look: clipping, indentation, folding and prose stay in the tool that shows them.

## 3. Not solved

- A heading's section is remembered by its heading line, so its end is a line no tool may name.
- A language is chosen by suffix alone: two languages sharing one suffix cannot be told apart.
- A diagnostic reaches the write it belongs to by the name its checker gives that file; a checker
  that names files unlike its own `shown` cannot be placed.
