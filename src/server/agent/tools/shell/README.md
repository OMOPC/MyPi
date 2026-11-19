# shell

`bash`, `alias` and `cd`: the tools that reach a command line and move the session's footing.

## 1. Design / Schema

| file | holds |
|---|---|
| `bash/tool.rs` | the `bash` tool |
| `bash/admit.rs` | `admit`: aliases → `#ref` → guard, and the line it hands the runner |
| `bash/segment.rs` | the separator set and the segment heads; the guard and the alias expansion share both |
| `bash/guard.rs` | the command classifier: `normalize`, `classify`, `Verdict`, `RuleHit` |
| `bash/shell.rs` | one command line: `run`, `run_to`, `OutputSink`, `Exit`, `StopFlag` |
| `cd/tool.rs` | `cd` |
| `alias/tool.rs` | `alias` add/list/remove; the config layer is read-only here |

```text
admit:  expand_alias  →  rewrite #<ref>  →  guard::classify     then  shell::run
```

`admit` is the only way raw text becomes a spawnable line; `bash` and `kairos` both call it. `guard`
sees the final command. It returns `Allow`, `Warn(ids)` or `Block(ids)`; a blocked command never
starts. `kairos` admits at tool-call time and freezes the admitted line, so a job exists only for a
command that already passed. `alias add` runs the same classifier on the command it stores.

One call may carry several lines; the shell runs them in order. A bare newline is a separator to the
guard, a backslash before a newline is a continuation, and `expand_alias` rewrites the head word of
every segment once.

The command itself runs in the session's shell (`Env.bash.shell`): `bash` or `zsh` on unix, `pwsh`
or `niu` on windows. `platform::shell` builds the command line and the quoted path a `#<ref>`
becomes; `platform::proc` starts it as a tree of its own and stops it as a whole; the platform picks
the ladder. A stop arrives through the `Stop` the runner registered after the spawn; the tool thread
itself only receives, so nothing wakes it on a timer. Stdin is the null device: a command that
prompts reads EOF, never the terminal the front end owns.

## 2. Design intent

- One gate for every command: the guard judges the text that actually runs, after aliases and
  `#ref` are resolved, so no path around it exists.
- `cd` moves the shared cwd the whole session reads; it holds no lock of its own.

## 3. Unresolved

1. The guard belongs to the unix shells only; `pwsh` and `niu` run the line as written.

## 4. Config fields

| key | meaning |
|---|---|
| `config.server.agent.tools.bashTimeout` | one shell command's deadline; default 600 s |
