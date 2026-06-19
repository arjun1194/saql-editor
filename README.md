# saql-editor

A small web workbench for the [SAQL engine](https://github.com/arjun1194/saql):
upload JSON/CSV files and query them in the browser. A thin Rust (axum) server
links `saql-core` + `saql-connectors` in-process — no data leaves your machine,
and there is no build step for the frontend.

This is a **separate** project from the engine. It depends on the engine via
path deps to a sibling `../saql` checkout (see `Cargo.toml` for the git-dep
alternative if you want to build it standalone).

## Run

```sh
cargo run        # serves http://127.0.0.1:7878
```

Then open the URL, upload a `.json` or `.csv` file (it becomes a named table),
and run SQL against it.

## How it works

- `POST /api/upload` — multipart (`file`, optional `name`, optional `kind`).
  The bytes are decoded by `saql-connectors` into an in-memory Arrow table held
  in a shared `Session`.
- `POST /api/query` — `{ "sql": "..." }` → `{ columns, rows, num_rows }`.
  Results are formatted to strings via Arrow's display kernel (any column type
  renders), capped at 5000 rows for the browser.
- `GET /api/tables` — the attached tables and their schemas.
- `GET /*` — the static UI from `web/`.

## Notes

- arrow-json infers all-quoted JSON values as text, so compare against strings
  (`WHERE position = '2'`) until the engine gains `CAST`. CSV columns are
  type-inferred, so numeric predicates work directly.
- The engine currently supports `SELECT` / `WHERE` (`= <> < <= > >=`, `AND`,
  `OR`) / projection + aliases / `LIMIT`. `JOIN`, `GROUP BY`, aggregates, and
  `CAST` are on the engine roadmap.

## Roadmap for the editor

Syntax highlighting + autocomplete (Monaco), drag-and-drop upload, result
export (CSV/JSON/Parquet via Arrow IPC writers), and eventually a client-side
WASM build of the engine for zero-server operation.
