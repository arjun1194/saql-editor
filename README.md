# SAQL Editor — a browser playground for SQL on your files

**Drop a JSON or CSV file into your browser and query it with SQL.** SAQL Editor is a small web app for the [SAQL engine](https://github.com/arjun1194/saql): upload a file, write a `SELECT` in a real code editor (with autocomplete and live error‑checking), and get a results table back — all running locally on your machine.

> It's the friendly, point‑and‑click way to use SAQL. No command line required.

## What you get

- 📤 **Upload** a JSON or CSV file — it becomes a table you can query.
- ⌨️ **A real code editor** (Monaco — the same one that powers VS Code) with SQL highlighting.
- 💡 **Smart autocomplete** — it reads your file's columns and suggests table and column names (correctly typed) as you go.
- 🔴 **Live error‑checking** — invalid SQL is underlined in red while you type.
- 📊 **Results grid** — run the query and see the rows immediately.

Everything runs **in‑process and on your machine** — your data is never uploaded anywhere.

## Run it

**Easiest — the standalone binary.** Download `saql-web` from the [Releases page](https://github.com/arjun1194/saql-editor/releases) (macOS Apple Silicon), then:
```bash
saql-web                 # open http://127.0.0.1:7878
PORT=9000 saql-web       # …or pick a different port
```
The binary is fully self‑contained — the web UI is baked in, so it runs from anywhere.

**From source** (any platform — needs [Rust](https://rustup.rs) and a checkout of the [engine repo](https://github.com/arjun1194/saql) beside this one):
```bash
git clone https://github.com/arjun1194/saql            # the engine (sibling dir)
git clone https://github.com/arjun1194/saql-editor
cd saql-editor
cargo run                # open http://127.0.0.1:7878
```

## How to use it

1. Open **http://127.0.0.1:7878**.
2. **Upload** a `.json` or `.csv` file — it appears in the table list with its columns.
3. Type a query, e.g. `SELECT * FROM mytable LIMIT 20`. Use the autocomplete as you type.
4. Hit **Run** to see the results.

The engine behind it speaks real SQL — `JOIN`, `GROUP BY`, aggregates (`COUNT`/`SUM`/`AVG`/`MIN`/`MAX`), `HAVING`, `ORDER BY`, and more. (Tip: text values use single quotes and column names with spaces use double quotes — `WHERE "First Name" = 'Heather'`.)

## How it's built

A tiny [axum](https://github.com/tokio-rs/axum) web server that links the SAQL engine **directly in‑process** (no separate database). Uploaded files are decoded into in‑memory Arrow tables held in a shared session.

| Endpoint | Purpose |
|---|---|
| `POST /api/upload` | Decode an uploaded JSON/CSV file into an in‑memory table. |
| `POST /api/query` | Run a SQL query, return `{ columns, rows }` (capped at 5000 rows for the browser). |
| `POST /api/validate` | Parse‑only check that powers the live red‑underline error‑checking. |
| `GET /api/tables` | List loaded tables + their columns (feeds autocomplete). |

The front‑end is plain HTML/CSS/JS with Monaco from a CDN — **no build step**. All three UI files are embedded into the binary, so `saql-web` ships as a single executable.

## Future scope

The editor grows alongside the [engine's roadmap](https://github.com/arjun1194/saql#future-scope). On the editor side specifically:

- 🗄️ **Connect to databases** — a "Connect" dialog for **PostgreSQL, MySQL, SQLite, DuckDB**, so you can browse and query live tables next to your uploaded files.
- 📁 **File‑system browser** — open a folder of CSV/JSON files and query across them; drag‑and‑drop a whole directory.
- 🔗 **GraphQL** — run your saved queries through a **GraphQL endpoint**, and add **GraphQL APIs** as a data source you can `SELECT` from.
- 🌐 **REST/HTTP sources** — paste an API URL and query its JSON like a table.
- 📦 **More formats** — Parquet, TSV, NDJSON, Excel uploads.
- 💾 **Save & share** — named saved queries, shareable links, and **export results** to CSV/JSON/Parquet.
- 🧠 **Smarter editing** — semantic validation (unknown‑column errors, not just syntax), inline result previews, and query history.
- 🚀 **Run fully in the browser** — compile the engine to WebAssembly so queries execute client‑side with zero server.

---

Built on the SAQL engine: **https://github.com/arjun1194/saql**
