# memo, todo, context

The record tools: `memo` stores notes, `todo` maintains one list, and `context` reads the history
back. All three answer a structured payload the front end renders, and the tool itself holds no
database handle.

## 1. Design / Schema

### memo

| field | value |
|---|---|
| `type` | `add`, `mod`, `del`, `show`, `history`, `list` |
| `scope` | `current` or `global`, default `current` |
| `name`, `content` | the note |

`add`, `mod` and `del` are `current` only; a `global` write is refused. `show`, `history` and
`list` take both scopes.

A memo write returns placeholder text and one `Memo` effect; the loop lands it in the same commit
as the result block, so the tool itself never stores a note. Reads go through `Source::memo`.

### todo

| field | value |
|---|---|
| `type` | `set`, `tick`, `view` |
| `items` | the whole list; `set` replaces it, and `items: []` clears it |
| `ids` | item numbers, 1-based |
| `done` | default `true`; `false` clears the tick |

`set` replaces the list, `tick` marks item numbers done or clears the tick, and `view` prints the
list with those numbers. Numbers are positions; an out-of-range number drops the whole call and
nothing changes.

A todo result's memo carries the whole list, so a front end maintains it by replacement. A
fully-ticked list stays until the next `set`; nothing clears itself.

### context

| field | value |
|---|---|
| `type` | `query`, `show` |
| `anchor` | a `#<ref>` hash or a block id |
| `direction` | `before` or `after`, default `after` |
| `limit` | default 3 |
| `tool` | a name or `all`, default `bash` |
| `ids` | the blocks `show` names |

`query` needs `anchor` and `direction`; `tool` is one name or `all`. `anchor` takes a `#<ref>` (the
hash printed when a result was truncated) or a block id. The answer is a JSON array of
`{id, tool, ok, summary}`, one element per recorded call.

`show` answers a JSON array of `{id, request, result}`; request and result are the stored text
parsed back into JSON, `null` when the block carries none.

Past ten rows, or past three shown blocks, the text lands in `Export::dir()` (`<temp>/mypi`) and
the reply names the file. The name carries the session and the parameters, so the same query in
one session overwrites its own file and no two sessions collide. The memo still carries every row.

### Query answers

The tools that read history answer JSON, and the same value rides along as the memo: `context`
`query`/`show`, `memo` `show`/`history`/`list`. The front end draws what the memo carries and does
not parse the text.

Each tool declares a `summary` naming the call in a history listing. It carries only the least
that locates the call: the operation for `memo`, `todo` and `context`, plus the name or anchor
when the call names one.

## 2. Design intent

- **The tool describes, the loop stores.** A memo write is one effect landed with its result
  block; a tool never opens the database.
- **One list, replaced whole.** A todo memo carries the entire list, so a front end never merges a
  diff or guesses which item moved.
- **History is read back, not kept.** `context` reads through `Source`; a long answer becomes a
  file rather than an unbounded block.

## 3. Unresolved

None.

## 4. Config fields

None. The three tools read no key of their own.