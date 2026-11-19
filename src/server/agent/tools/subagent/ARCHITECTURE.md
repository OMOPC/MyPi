# subagent — architecture

## Files

| file | holds |
|---|---|
| `mod.rs` | declarations and re-exports (`TASK`, `FINAL`), and `project_dir` |
| `task.rs` | the `task` tool: the call's project folder, template, variables, answer name, the child's model, the `Child` effect |
| `final.rs` | the `final` tool: the child's draft, its edits, the one submit |
| `template.rs` | template lookup (project, then global, then the named path), front matter, `{{var}}` rendering, roster subset checks, the stem |
| `answer.rs` | the answer file: the first free name inside the project's `answers` folder, taken against the names one call already handed out, and the one stamped write |

## Relations

- `task` depends on `contract::env` for the child roster (`Models`, `roster`) and on
  `contract::output::Effect::Child`; the loop writes the session row and books the child.
- `task` depends on `subagent::template` and `subagent::answer` for the project folder, the
  variables and the free name.
- `final` depends on `Source::child` for the child's draft and on `answer.rs` for the write.
- The loop-side runner books the child, nudges it three times when it ends without a submit, and
  drops it; the child removals live in `loop/job`.
- `api::post::init` drops `final` from a main session's list and manuals.
- Consumed by the engine and the registry like every tool.
- Kept in step with `loop/turn/cycle`: the child list is corrected once when the child is spawned —
  `task` out, `final` in.

## Must stay in step

1. `subagent::template` and the parent roster: a template may only name tools the parent has.
2. `answer.rs` and the `task` call's item list: every name is allocated against the names the same
   call already handed out.
3. The `Child` effect and `loop/job`: the tool describes the child; the loop creates the session
   row and the runner.
4. `api::post::init` and `final`: a main session never receives the tool, so its manuals must not
   name it either.