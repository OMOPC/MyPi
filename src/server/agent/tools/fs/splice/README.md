# splice

Whole lines replaced by number: many ranges of one file in one call, one snapshot, one write. A
range names itself by what it lands on, so a line the model counted wrong is refused instead of
written somewhere else.

## 1. Design / Schema

### One call

`path` and `edits`, and `unfold` when the batch's check report is wanted whole. Each element of
`edits` is one range `{start, end, new}`; the elements are answered in the order they were given,
and a batch of calls on one file becomes one write.

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

### What holds a range back

| refused | why |
|---|---|
| a line that is not what the anchor says | the model read a different file, or a stale one; `read` or `grep` again and resend |
| two ranges of one call overlapping | the model contradicting itself; the whole call is left alone |
| a range overlapping one from an earlier call in the batch | the earlier call is written, the later one is refused, and the rest land |
| a file not read in this conversation | no licence to write it |
| a file changed on disk since `read` described it | the anchor's coordinates moved; a fresh read is the way back |
| a licence revoked by a refused write | the next write needs a fresh read |

The lines are judged before the anchors: a range that fights another over them is refused for the
fight, and only a range whose lines are its own is asked whether it still finds what it named. So
two refusals never stack on one range, and every refused range still owns the lines it names.

A refusal takes one range, never the call — except two of the call's own ranges overlapping, which
leaves that call alone. The verdict counts what landed and what did not, and a refusal's reason
stands under its own block.

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
file can be read as one. A text the formatter could not parse was never formatted, so each change's
landing is the one its own range puts it at.

Every parse is one `rustfmt` run over stdin: the binary is looked for beside this executable first,
then on PATH, and its edition comes from the nearest `Cargo.toml` above the file, read once per
directory. A verdict the formatter reached is remembered per edition and content; a timeout or a
missing binary is not, since neither says anything about the text.

When the merged text does not parse, each change is tried on its own against the file as it was, so
the answer can name the block the parser blames: a missing brace points below the change, never at
it. That trial is a report only; nothing is filtered out by it.

### The batch's audit

A written `.rs` file hands its shell and its landing places to `fs/fault.rs`, which checks the crate
once per batch and hangs every diagnostic under the block whose lines it hit. What no written call
can hold goes to the crate's list on the batch's last written answer that can still take lines;
`unfold: true` lifts that list's fold. A file the parser refused still reaches the checker.

### The answer

The shape is `grep`'s: a verdict line, then one block per edit, each opening with `path:line`, an
`in:` line naming the item it sits in, and numbered source. A block that stands there carries `+`;
a deletion shows what it removed with `-`. Nothing else marks a block or separates two of them.

The verdict is `OK`, `PARTLY FAILED` (with `, LINES SHIFTED` when a refused edit's lines moved),
`FAILED, NOTHING WRITTEN`, `WRITTEN, NOT PARSED`, or `WRITTEN, UNFORMATTED`, then the file's
length:

```text
[Splice] OK · now 11 lines

src/demo.rs:6-11
  in: pub fn alpha() -> u8
+   1|pub fn alpha() -> u8 {
+   2|    1
+   3|}
+   4|pub fn beta() -> u8 {
+   5|    2
+   6|}
src/demo.rs:3-5
-    |pub fn alpha() -> u8 {
-    |    1
-    |}
+    |(nothing: the range is gone)
```

A written edit is named where it landed and shows the block that stands there now; the plus side
counts inside the block. A deletion has no landing of its own: it is named by the range it took, and
shows what left, unnumbered. Both sides fold in the middle past twelve rows, and a line is cut at
140 characters.

A refused edit shows its reason and nothing else, under a heading of the line its own range now
starts on: the written ranges above it moved it, so the old number it was sent with is not where it
sits any more. A block the parser blames carries the parser's words under it:

```text
[Splice] PARTLY FAILED, LINES SHIFTED · now 13 lines

src/demo.rs:9-11
  in: pub fn beta
+   1|pub fn beta() -> u16 {
+   2|    2
+   3|}
src/demo.rs:14
  in: pub fn gamma
  ! OVERLAPPED: an earlier call on this file wrote these lines; Re-Read or Re-Grep, then resend
  ! line 3: mismatched types
```

A text that does not parse is written as it stands, and the answer says which edit the parser blames
or, failing that, what it said:

```text
[Splice] WRITTEN, NOT PARSED · now 13 lines

src/demo.rs:2
+   1|pub fn broken( -> u8 {
[i] the file does not parse: this file contains an unclosed delimiter
```

`WRITTEN, UNFORMATTED` is the same shape with a formatter that never answered:
`[i] not formatted, written as it stands: rustfmt did not finish in 5 s, so it was killed`.

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
killed and nothing reaches the file, so the call answers that the file is unchanged. After the
write, the change stands and waits for the batch's audit: the answer is finished once the batch's
last write has landed and the crate has been checked.

## 2. Design intent

**A line number is useless without an anchor.** A number says where a change starts only if the
model's picture of the file is still true; the words a line opens with are the proof it is, and they
cost a few bytes instead of a whole `old` block. That is why this tool exists beside `edit`: it
spends `read`'s line numbers instead of fighting them.

**One anchor rule for both ends.** `start` and `end` read the same way, so a range is never a
number on one side and a quotation on the other; the ends that have no line to name — an insert's
`end`, an append's `start` — say so by carrying none.

**A refusal is placed, not guessed.** A refused range still owns the lines it named, so where it
starts after the write is arithmetic over the written ranges above it — never a number carried over
from the snapshot, and never a net sum that lets an insert and a cut cancel.
**A refusal is local.** One stale range is dropped and the rest of the batch lands; a call whose
own ranges contradict each other is left alone, since there is no way to tell which one the model
meant. Either way the verdict names what landed, and the model resends only what lost.

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
language, and the batch's check report from `unfold`.
