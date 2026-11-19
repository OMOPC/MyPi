# cd

`cd` gives the session a new footing: it moves the one cwd handle every tool reads.

## 1. Design / Schema

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

## 2. Design intent

- **A move, not a shell setting.** `cd` writes the same `Workspace` handle the api's `/cdp`
  writes, so a move lands for every tool at once.
- **One standing rule.** A session never holds a directory it cannot enter.

## 3. Unresolved

None.

## 4. Config fields

None.