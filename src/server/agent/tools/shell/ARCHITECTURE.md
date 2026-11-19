# shell — architecture

## Files

| file | holds |
|---|---|
| `mod.rs` | declares the shell family |
| `bash/mod.rs` | declares the command tool, the admission gate, the segment scanner, the guard and the runner |
| `bash/tool.rs` | the `bash` tool |
| `bash/admit.rs` | `admit`: aliases → `#ref` → guard, and the line it hands the runner |
| `bash/segment.rs` | the separator set and the segment heads; the guard and the alias expansion share both |
| `bash/guard.rs` | the command classifier: `normalize`, `classify`, `Verdict`, `RuleHit` |
| `bash/shell.rs` | one command line: `run`, `run_to`, `OutputSink`, `Exit`, `StopFlag`; registers the stop action, spawns and signals through `platform::proc` |
| `cd/{mod,tool}.rs` | `cd` |
| `alias/{mod,tool}.rs` | `alias` add/list/remove; the config layer is read-only here |

## Relations

- Depends on `platform::shell` for the session's shell and the one-shot argument shape, on
  `platform::proc` for the process group, and on `platform::stop` for the stop action.
- Depends on `workspace` for the shared cwd handle: `cd` writes the same handle the api's `/cdp`
  writes.
- Depends on `contract::env` for `BashEnv` (program, timeout), `Aliases` and the licensed zones.
- `bash/admit` and `bash/guard` are shared with `kairos`: every job command goes through the same
  admission and the same classifier.
- `alias add` runs the guard on the command it stores, so the same classifier has two callers.
- Consumed by the engine and the registry like every tool.

## Must stay in step

1. `bash` and `kairos`: every command goes through `admit`; the guard runs once, on the final text.
2. `admit`'s alias expansion and `alias add`'s stored command: both use the one segment scanner.
3. `BashEnv.timeout` and `config.server.agent.tools.bashTimeout`: the loop fills the value, and the
   tool reports the deadline through `ToolOutput::failure`.
4. `cd` and the api's `/cdp`: one `Workspace` handle, so a move lands for both at once; the two
   standing rules must agree on `may_stand_in`.
5. The guard and the unix shells: `pwsh` and `niu` run the line as written; a change to the guard
   does not change them.