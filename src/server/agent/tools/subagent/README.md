# subagent

`task` starts a child session from a template and returns; `final` is how that child hands its work
back. A child is a session: it reuses the loop, storage and stream, and its tool list is corrected
once when it is spawned.

## 1. Design / Schema

### task

| field | value |
|---|---|
| `tasks` | one item per child, with `template`, `vars` (object of strings), `cwd`, `maxToolCalls`, `name`, `model` |
| `list` | default `false`; `true` prints the model roster instead of spawning |

`list` is a query operation and refuses an array. Two children with one stem get `probe` and
`probe-1`, not the same answer file: every answer name is allocated against the ones the same call
already handed out.

`model` names one `provider:model` from the roster the loop copied out of `models.yml` when it was
built; an unknown address is refused with the roster printed. A child without `model` runs on its
parent's model.

### final

| field | value |
|---|---|
| `type` | `write`, `edit`, `read`, `submit` |
| `content`, `old`, `new`, `range` | the draft operation |

`final submit` writes the answer file and is the only thing that ends a child. It is not a stop:
its result carries `stop`, and the turn settles as `finished`.

`final` is appended to every child's list; the parent does not name it and does not know it exists.
A main session never resolves it at all.

### The child's project

- The project folder is `<the parent's cwd>/.mypi`, taken where the `task` call is made: a child's
  own `cwd` never moves it, and a `cd` before the call does. Nothing walks up to a parent folder.
- Templates come from `<project>/agents`, then the user's global agents folder, then the path the
  call names.
- The answer file is `<project>/answers/<name>.md`, created when the first child of that project is
  spawned; the name is fixed at spawn, from `name` or the template's stem, with `-1`, `-2` when a
  file or another item of the same call already holds it.
- A child takes no user messages. `task` is the only tool a child cannot hold; `final` is the only
  one it never has to ask for.
- A template may only name tools the parent has.

### Answer names and summaries

`task`'s history summary names the first template and its answer name. `final`'s names the
operation.

## 2. Design intent

- **A child is a session.** Subagents reuse the loop, storage and stream; corrected lists keep a
  child from spawning children of its own.
- **Templates plus variables.** A template carries the child's tools and prose; the call fills only
  the variables, so the model does not restate the whole instruction.
- **One answer file, one submit.** A child's work is not finished until `final submit` writes the
  file; a child that ends without one is nudged and then dropped.

## 3. Unresolved

1. Template front matter reads `tools` only.

## 4. Config fields

| key | use |
|---|---|
| `config.server.agent.subagent.maxToolCalls` | one child's tool-call budget; read by the loop |
| `config.server.agent.loop.tools.task.timeoutSecs` | how long one sub-session may run; default 3600 s |