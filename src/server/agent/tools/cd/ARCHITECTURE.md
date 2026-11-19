# cd — architecture

## Files

| file | holds |
|---|---|
| `mod.rs` | declares the tool |
| `tool.rs` | `cd`: read the path, resolve it against the shared cwd, write the new cwd |

## Relations

- Depends on `workspace` for the shared cwd handle and `may_stand_in`.
- The api's `/cdp` writes the same handle, so a move lands for both at once.
- `standing_dir` in the loop uses the same rule when a session is booked or bound.

## Must stay in step

1. `cd` and the api's `/cdp`: one `Workspace` handle and one `may_stand_in`.
2. `cd` and `standing_dir`: a launch directory that cannot stand lands in the temp dir.
