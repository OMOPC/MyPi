# shell

`bash`, `alias` and `cd`: the tools that reach a command line and move the session's footing.

## 1. Design / Schema

### Files

| file | holds |
|---|---|
| `bash/tool.rs` | the `bash` tool |
| `bash/admit.rs` | `admit`: aliases → `#ref` → guard, and the line it hands the runner |
| `bash/segment.rs` | the separator set and the segment heads; the guard and the alias expansion share both |
| `bash/guard.rs` | the command classifier: `normalize`, `classify`, `Verdict`, `RuleHit` |
| `bash/shell.rs` | one command line: `run`, `run_to`, `OutputSink`, `Exit`, `StopFlag` |
| `cd/tool.rs` | `cd` |
| `alias/tool.rs` | `alias` add/list/remove; the config layer is read-only here |

### One command

```text
admit:  expand_alias  →  rewrite #<ref>  →  guard::classify     then  shell::run
```

`admit` is the only way raw text becomes a spawnable line; `bash` and `kairos` both call it. `guard`
sees the final command. It returns `Allow`, `Warn(ids)` or `Block(ids)`; a blocked command never
starts. `kairos` admits at tool-call time and freezes the admitted line, so a job exists only for a
command that already passed. `alias add` runs the same classifier on the command it stores.

One call may carry several lines; the shell runs them in order. A bare newline is a separator to
the guard, a backslash before a newline is a continuation, and `expand_alias` rewrites the head word
of every segment once.

The command itself runs in the session's shell (`Env.bash.shell`): `bash` or `zsh` on unix, `pwsh`
or `niu` (niubash) on windows. `platform::shell` builds the command line and the quoted path a
`#<ref>` becomes; `platform::proc` starts it as a tree of its own and stops it as a whole; the
platform picks the ladder. A stop arrives through the `Stop` the runner registered after the spawn; the tool thread
itself only receives, so nothing wakes it on a timer. Stdin is the null device: a command that
prompts reads EOF, never the terminal the front end owns.

### bash

| field | value |
|---|---|
| `command` | always; one or more lines |

`bash` takes no array. A command's own deadline is the session's `bashTimeout`; a command that
runs past it reports `timed out after Ns` through `ToolOutput::failure`, which is not a stop.

### cd

| field | value |
|---|---|
| `path` | always |

`cd` is licensed by one rule, `may_stand_in`: not the filesystem root, not a top-level directory
(`/tmp` and its children excepted), and nowhere under the home tree. Everything deeper is fair
ground, and the licensed zones then follow `cwd`.

Where a session may stand is asked in two places only: `init` and `bind` land a barred or missing
launch directory in the temp dir (`standing_dir`: it must exist, be a directory, and pass
`may_stand_in`; a row whose directory has since gone lands there too), and `cd` refuses it
outright. `/cdp` asks only whether the directory exists.

`cd` lands its result with `memo {kind: "cwd", path, from}`: the history card relativizes from it
and the front end marks the directory as a temporary move. It holds no lock of its own.

### alias

| field | value |
|---|---|
| `type` | `add`, `list`, `del` |
| `aliases` | `add`: one item per alias, with `name` and `command` |
| `names` | `del`: the names to remove |

`alias` expands the head word of every command segment, once, and adds no variables, functions or
multi-line bodies. A name a session writes is one word the scanner accepts as a segment head; a
name the config fixes is read-only for the session. `alias add` runs the guard on the command it
stores.

`add` takes an array of aliases; `del` takes `names` as a text list; `list` is a query operation
and refuses an array. `list` answers JSON, and the same value rides along as the memo.

## 2. Design intent

- **One gate for every command.** The guard judges the text that actually runs, after aliases and
  `#ref` are resolved, so no path around it exists.
- **A guard, not a sandbox.** The model runs real shell commands; the classifier blocks the
  machine-destroying shapes and warns on the rest, and it always judges the expanded command.
- **Two questions, two rules.** File tools may touch the session's directory and the temp
  directory; `cd` answers a different one — where may a session stand at all — and its rule is a
  blacklist, not a zone list.
- **One shared cwd.** `cd` moves the handle the whole session reads, including the api's `/cdp`.

## 3. Unresolved

1. The guard belongs to the unix shells only; `pwsh` and `niu` run the line as written.

## 4. Config fields

| key | meaning |
|---|---|
| `config.server.agent.tools.bashTimeout` | one shell command's deadline; default 600 s |
| `config.server.agent.tools.aliases` | user aliases as `name=command`; every session reads them, nothing shadows them |

The shell program itself comes from `Env.bash.shell`; the loop fills it when it books a session.