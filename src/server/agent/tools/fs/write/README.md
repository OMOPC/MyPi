# write

`write` replaces a whole file and refuses nothing: it is the counterpart of `edit`, which refuses
an ambiguous match. The text that lands is the formatter's when the formatter reached a verdict,
and the text as sent when it did not.

## 1. Design / Schema

### One call

`path` and `content`. The file is overwritten whole. A `.rs` file goes through the formatter first,
and the formatted text is what lands.

A text the parser refuses still lands as it came, the answer says the file does not parse, and the
batch's check still runs — the report names the file again. A formatter that did not finish or
could not run says nothing about the text, so the content lands as it came, the answer says it was
not formatted, and the type check still runs: `cargo check` does not need rustfmt.

`write` takes no array: its payload is large enough that a batch would move the same bytes into one
argument.

### The batch's audit

A written file joins the batch's audit: each of its errors and warnings stands under the answer as
a source seam, once, and the crate's list carries only what no written call could hold. A stop
before the write leaves the file untouched and the answer says so; nothing is said about the
compiler when there is nothing wrong.

### The answer

```text
overwrote /p/src/foo.rs (2311 bytes)
```

## 2. Design intent

- **Whole-file replacement is unconditional.** A model that knows the whole file needs no anchor;
  the formatter and the checker still judge what landed.
- **Nothing is withheld.** A parse failure and a missing formatter are reported, but the content
  still lands, because the model's next check is the real verdict.
- **The check does not depend on the formatter.** `cargo check` runs whether or not rustfmt was
  available, so a missing toolchain program never hides a type error.

## 3. Unresolved

None.

## 4. Config fields

None. `write` reads no key of its own; the formatter's and the checker's behaviour comes from the
language, and the batch's reports from `unfold`.