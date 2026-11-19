# splice

Whole lines replaced by number: many ranges of one file in one call, one snapshot, one write. A
range names itself by what it lands on, so a line the model counted wrong is refused instead of
written somewhere else.

## 1. Design / Schema

### One call

`path` and `edits`, and `unfold` when the batch's check and test reports are wanted whole. Each
element of `edits` is one range `{start, end, new}`; the elements are answered in the order they
were given, and a batch of calls on one file becomes one write.

| field | value |
|---|---|
| `start` | the range's first line, 1-based, then the words that line opens with |
| `end` | the range's last line, inclusive, read the same way; its words are checked when the range covers more than one line. `start - 1` inserts before `start` and names no line |
| `new` | what stands in the range's place, any number of lines; empty deletes the range |

An insert appends when `start` is one past the last line, deletes when `new` is empty, and opens or
closes a block like any other change.

### An anchor says what it lands on

`start` and `end` read as a line number and then the words that line opens with, `12 pub fn run`.
The words are compared as a flat prefix — whitespace runs folded to one space, the line walked from
its front — so a model may stop short of the whole line and cannot match the middle of one.

The `end` words are checked only when the range covers more than one line: a single line is
confirmed by its `start`, and an insert has no end line at all.

A bare line number is read as a blank line and nothing else: on a dense line it is refused, and the
refusal quotes what the line actually reads and the shape to name it by. Two lines have no words to
give and take a bare number: a blank line, and the end of the file, which an append names.

A `@@CUT(a..b)@@` inside `new` carries no words of its own, because the macro names its lines.

### Authorisation

A file may be spliced only after `read` or `grep` has shown it in this conversation. That showing is
the authorisation, and it carries the content's hash: a file that changed on disk since is
unauthorized, and so is a file whose last splice did not end `OK`.

Any splice that is not a clean `OK` — a refused range, an unreadable argument, a text that does not
parse — takes the authorisation away at once, one missed range out of many included. The model's way
back is one: `read` or `grep` the file again, then send the edit. A formatter that never ran is not
the text's fault, so `WRITTEN, UNFORMATTED` keeps the authorisation.

### What holds a range back

| refused | why |
|---|---|
| a line that is not what the anchor says | the model read a different file, or a stale one; that range is left out and the rest land |
| a range overlapping one already accepted, in the same call or an earlier one | the later range loses; the earlier one is written and the rest land |
| a range that cannot exist: past the last line, `end` above `start - 1`, a malformed `@@…@@`, two `@@CUT` of one call cutting the same line | that item is left out; the rest land |
| a file not read or grepped in this conversation | unauthorized |
| a file changed on disk since it was read | unauthorized |

Overlap is judged first and alone: a range that loses to an accepted one carries no other reason.
A range whose lines are its own is then asked whether it still finds what it named. Every refusal
takes one range or one item, never the call and never the batch: the rest of it lands, and the
verdict counts what did.

Two ranges that only touch are both written: half-open ranges, and an insertion belongs to neither
side of a boundary. An insertion on a range's own first line lands above that range's block.

### The batch's one write

Every `splice` call of one message on one file is merged: the ranges arrive in call order, are
measured against the file as it stood before the message, and land in one atomic write. A later
call's range that overlaps an earlier call's landing loses, because what the earlier call wrote is
no longer what the later call's words describe.

The per-path write lock of `workspace` is held across the whole of it: the file's bytes are read,
compared with the licence, and written back inside one lock, so no other call sees a half-applied
batch.

### Filling macros

`new` may name lines of the file instead of repeating them. `@@COPY(a..b)@@` stands for lines `a`
to `b` of the snapshot, 1-based and inclusive; `@@CUT(a..b)@@` stands for the same lines and adds
the change that removes them from where they were. Filling happens once, before any range is
applied, so a token inside the copied lines is copied rather than followed. A macro may sit
anywhere in `new`; a range a macro names is cut once per call, and a change may not cut the range it
replaces.

### The formatter and the checker

Both are the file's language's own; a language that names no formatter writes the text as it stands.

| language | what happens |
|---|---|
| one with a formatter | the merged text goes through it in one pass; its layout is what lands |
| one whose formatter says the text does not parse | the text lands as it stands and the answer carries the parser's words |
| one whose formatter does not finish or cannot run | the text lands as it stands and the answer says it was not formatted |
| one without a formatter | the text is spliced from its lines and nothing else |

Each change is wrapped in a marker comment before the formatter sees it, so the answer can say
where that block ended up; the markers are stripped from what is written, and the blank lines they
held open are folded back. A comment's word carries a fingerprint of the snapshot, so no line of the
file can be read as one. A landing the markers cannot be walked back to falls to the position the
plain splice would give it.

Every parse is one `rustfmt` run over stdin: the binary is looked for beside this executable first,
then on PATH, and its edition comes from the nearest `Cargo.toml` above the file, read once per
directory. A verdict the formatter reached is remembered per edition and content; a timeout or a
missing binary is not, since neither says anything about the text.

When the merged text does not parse, each change is tried on its own against the file as it was, so
the answer can name the block the parser blames: a missing brace points below the change, never at
it. That trial is a report only; nothing is filtered out by it.

### The batch's audit

A written `.rs` file hands its shell and its landing places to `fs/fault.rs`, which checks the crate
once per batch and hangs every diagnostic under the block whose lines it hit. A test failure is
listed above a check warning on the same block. What no written call can hold goes to the crate's
list on the batch's last written answer that can still take lines;
`unfold: true` lifts that list's fold. A file the parser refused still reaches the checker.

### The answer

Three parts, and nothing outside them. The header names the file and says what became of the call;
the body is one block per edit; the footer is the batch's reports and the next move.

A call that landed whole and clean draws the header and nothing else — `splice src/demo.rs: OK`.
Its diff is what the model just sent, and the file it can read is the truth; the body exists for
calls that partly failed.

```text
splice src/demo.rs: PARTLY FAILED

src/demo.rs:6-11
  in: impl Foo > pub fn alpha
+  6|pub fn alpha() -> u8 {
+  7|    1
+  8|}

src/demo.rs:9-11
  in: pub fn beta
-  9|pub fn beta() -> u8 {
- 10|    2
- 11|}

OVERLAP

SHIFTED, the edit scope you requested earlier is now 14-16; reread or regrep to issue another splice
  ! MISMATCH: line 12 reads `pub fn gamma`, not `pub fn delta`
```

The header is one line: `splice <path>: <verdict>`. The verdict is `OK`, `PARTLY FAILED`,
`FAILED, NOTHING WRITTEN`, `WRITTEN, NOT PARSED` or `WRITTEN, UNFORMATTED`. A call that never
reached its edits names the reason instead of a verdict:

```text
splice src/demo.rs: FAILED, NOTHING WRITTEN

! `src/demo.rs` is unauthorized: it has not been read in this conversation
```

A written block opens with where it landed, names the item, and numbers its lines by the file's own
numbers — the plus side as it now stands, the minus side as it stood before the write. A deletion
has no plus side. A block that lost for overlap says one word:

```text
OVERLAP
```

A block refused by its anchor says where the lines it asked for now sit, then the anchor's words:

```text
SHIFTED, the edit scope you requested earlier is now 14-16; reread or regrep to issue another splice
  ! MISMATCH: line 12 reads `pub fn gamma`, not `pub fn delta`
```

`UNCHANGED, ` replaces `SHIFTED, ` when nothing was written and the range therefore did not move.
An item that could not be read at all has no range to name and carries its reason alone.

Both sides fold in the middle past twelve lines, counted as `[+N lines]`. No line is cut.

The footer follows the body one blank line down and holds only what the file itself cannot say:

```text
[i] check · mypi: 3 errors, 2 warns
  src/lib.rs:40: mismatched types | help: expected `u8`, found `u16`
[+9 more faults: set `unfold` true to list them]
[i] test · mypi: 4 passed, 1 failed
  tests/flow.rs:12: assertion failed
[i] fmt · src/demo.rs does not parse, written as it stands: unclosed delimiter
```

The check's counts cover every diagnostic the crate's check found, whether it hangs under an edit
or is listed here. `unfold` lists them all instead of the first twelve.

### The memo

One segment per range, in the call's order: the lines it took and the lines that stand there now,
numbered on both sides. A range that was left out carries positions and a reason, never lines.

```text
{ "segments": [ { "edit": 1, "state": "applied", "fromLine": 11, "atLine": 11,
                  "old": "<lines that went>", "new": "<lines that took their place>" }, … ] }
```

A range left out carries no lines at all: its `fromLine` is the line it named, its `atLine` the
line its own range starts on now, and `error` what stopped it. A `@@CUT(a..b)@@` adds a segment of
its own — the lines it removed, with an empty `new`, under the same edit number. Segments that are
not neighbours are drawn apart, and the card folds what it does not need. The new line numbers are
the formatter's, so the tool carries them rather than leaving the front end to guess.

### Stopping

A stop lands in one of two places, and the answer says which. Before the write, the formatter is
killed and nothing reaches the file, so the call answers `FAILED, NOTHING WRITTEN` and `interrupted`;
the file keeps its authorisation, since nothing about it changed. After the write, the change stands
and waits for the batch's audit: the answer is finished once the batch's last write has landed and
the crate has been checked, and a check that never ran says `[i] check · interrupted`.

## 2. Design intent

**A line number is useless without an anchor.** A number says where a change starts only if the
model's picture of the file is still true; the words a line opens with are the proof it is, and they
cost a few bytes instead of a whole `old` block. That is why this tool exists beside `edit`: it
spends `read`'s line numbers instead of fighting them.

**One anchor rule for both ends.** `start` and `end` read the same way, so a range is never a
number on one side and a quotation on the other; the ends that have no line to name — an insert's
`end`, an append's `start` — say so by carrying none.

**One rule for overlap.** Every range is judged against the ranges already accepted, whichever call
sent them. The later one loses, the earlier one lands, and nothing else about the call changes.
A special case per call would make the same mistake refuse different amounts of work.

**A refusal is placed, not guessed.** A refused range still owns the lines it named, and it shares
none with a written range, so where it starts after the write is arithmetic over the written ranges
above it — never a number carried over from the snapshot, and never a net sum that lets an insert
and a cut cancel.

**A refusal is local.** One stale range is dropped and the rest of the batch lands; the model resends
only what lost. Authorisation goes with it, because the model's picture of the file is what failed.

**The header says the verdict, the blocks say the place.** A successful call carries no numbers past
its blocks; a refused one carries exactly one sentence about where its lines went, and the same
sentence is the instruction to read the file again.

**The formatter reports, it does not withhold.** A text that does not parse still lands, because the
model's next `cargo check` is the real verdict; the tool only makes sure the model knows.

**One write per file per message.** Two calls on one file would each need a fresh read between them;
merging them makes the second call's coordinates mean what the model thought they meant.

## 3. Unresolved

1. A range cannot name a blank line by a number `read` prints: `read` prints no number on a blank
   line, so the model counts one itself.
2. `@@COPY` and `@@CUT` take plain line numbers, with no worded anchor of their own: a macro naming
   the wrong lines copies them without a refusal.
3. Nothing checks that a `new` block belongs where it is put beyond the parser: a block moved into
   another function parses and lands.

## 4. Config fields

None. `splice` reads no key of its own; the formatter's and the checker's behaviour comes from the
language, and the batch's reports from `unfold`.
