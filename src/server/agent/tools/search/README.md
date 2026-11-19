# search

The `search` tool entry: one query against exactly one provider, on the session's search tab. The
engines, the page walk and the hit parsing live in `src/web`; this tool is the model's verb.

## 1. Design / Schema

| field | value |
|---|---|
| `query` | always; the text to search for |
| `provider` | always; `google`, `bing` or `baidu` |
| `pages` | default 1; pages 1..=`pages` |

`provider` is `search`'s alone: one search runs against exactly one provider, so a call that names
none is refused rather than spread across several tabs. `search` takes no array.

See `src/web/README.md` for the engines, the page walk and the answer's shape.

## 2. Design intent

- **One query, one provider.** A search that names no provider is a question; spreading it across
  tabs would put three different result sets into one answer.
- **A separate tab.** Search and interactive work have different lifetimes; `search` holds the
  search tab, so a page being read is not replaced by a query.

## 3. Unresolved

See `src/web/README.md`; the tool layer adds none.

## 4. Config fields

None at the tool layer.