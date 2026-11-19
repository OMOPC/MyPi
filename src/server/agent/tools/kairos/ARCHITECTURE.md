# kairos — architecture

## Files

| file | holds |
|---|---|
| `mod.rs` | module declarations and the tool export |
| `tool.rs` | the `kairos` tool manual and parameters |
| `service.rs` | operation dispatch and rendering |
| `command.rs` | a line admitted at call time as a `JobRunner`: only `run_to` is left |
| `text.rs` | the text `JobRunner`: nothing to run, so only the clock makes it due |
| `duration.rs` | relative duration parse (`10min`, `1h30m`) |

## Relations

- Depends on `contract::job` for `JobSpec`, `JobHost` and the receipt shapes; the scheduler, the
  pool and the wakes live in `loop/job`.
- Depends on `shell/bash` for `admit` and the guard, so a job only exists for a command that
  already passed.
- Depends on `Source::jobs` for the session's `JobHost`; the loop attaches it when it books the
  session.
- Answers through the engine and the registry like every tool.
- Kept in step with `shell/bash`: every kairos command goes through `admit`, and the guard runs
  once on the final text.

## Must stay in step

1. `JobSpec` and `loop/job`: the port is the whole surface a tool sees; a new job field starts
   there.
2. `admit` and the frozen line: changing either the alias expansion or the guard changes what a
   job may run.
3. The query operations and the batch rule: `list`, `status`, `result` and `cancel` refuse an
   array, and the engine's item model expects one item for each.