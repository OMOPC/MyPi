# shell — architecture

## Files

| file | holds |
|---|---|
| `mod.rs` | declares the shell family |
| `tool.rs` | the `shell` tool |
| `admit.rs` | `admit`: aliases → `#ref` → guard, and the line it hands the runner |
| `segment.rs` | the separator set and the segment heads; the guard and the alias expansion share both |
| `guard.rs` | the command classifier: `normalize`, `classify`, `Verdict`, `RuleHit` |
| `runner.rs` | one command line: `run`, `run_to`, `OutputSink`, `Exit`, `StopFlag`; registers the stop action, spawns and signals through `platform::proc`, folds `\r\n` as it reads |
| `alias/{mod,tool}.rs` | `alias` add/list/remove; the config layer is read-only here |

## Relations

- Depends on `platform::shell` for the session's shell, the one-shot argument shape and the
  encoding it sets, on `platform::proc` for the process group, and on `platform::stop` for the
  stop action.
- Depends on `contract::env` for `ShellEnv` (backend, timeout) and `Aliases`.
- `admit` and `guard` are shared with `kairos`: every job command goes through the same admission
  and the same classifier.
- `alias add` runs the guard on the command it stores, so the same classifier has two callers.
- Consumed by the engine and the registry like every tool.

## Must stay in step

1. `shell` and `kairos`: every command goes through `admit`; the guard runs once, on the final text.
2. `admit`'s alias expansion and `alias add`'s stored command: both use the one segment scanner.
3. `ShellEnv.timeout` and `config.server.agent.tools.bashTimeout`: the loop fills the value, and the
   tool reports the deadline through `ToolOutput::failure`.
4. The guard and the unix shells: `pwsh` and `niu` run the line as written; a change to the guard
   does not change them.