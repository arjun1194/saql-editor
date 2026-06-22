//! WebAssembly bindings for the SAQL engine.
//!
//! These expose the same four operations the `saql-editor` server does
//! (`tables` / `upload` / `query` / `validate`) — but the engine is compiled to
//! WASM and runs **entirely in the browser**. An in-memory [`Session`] holds the
//! uploaded tables; there is no server and no network, so the data never leaves
//! the page. This is what lets the editor be hosted as a static site (GitHub
//! Pages) with a fully working query engine.
//!
//! Each function returns a JSON string (parsed by the page's JS); fallible ones
//! return `Result<_, JsValue>`, which surfaces as a thrown error in JS.

use std::cell::RefCell;

use arrow::record_batch::RecordBatch;
use arrow::util::display::{ArrayFormatter, FormatOptions};
use saql_core::Session;
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// Cap rows handed to the page so a huge result can't wedge the DOM.
const MAX_DISPLAY_ROWS: usize = 5000;

#[derive(Clone, Serialize)]
struct ColumnMeta {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}

#[derive(Clone, Serialize)]
struct TableMeta {
    name: String,
    kind: String,
    columns: Vec<ColumnMeta>,
    num_rows: usize,
}

#[derive(Serialize)]
struct QueryResp {
    columns: Vec<ColumnMeta>,
    rows: Vec<Vec<String>>,
    num_rows: usize,
    truncated: bool,
}

#[derive(Serialize)]
struct ValidateResp {
    valid: bool,
    message: Option<String>,
    line: Option<usize>,
    col: Option<usize>,
}

struct State {
    session: Session,
    tables: Vec<TableMeta>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        session: Session::new(),
        tables: Vec::new(),
    });
}

/// Runs automatically when the module loads: route panics to the JS console.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// The loaded tables, as JSON: `[{ name, kind, columns:[{name,type}], num_rows }]`.
#[wasm_bindgen]
pub fn tables() -> String {
    STATE.with(|s| serde_json::to_string(&s.borrow().tables).unwrap_or_else(|_| "[]".into()))
}

/// Decode an uploaded file and register it as a queryable table.
///
/// - `kind` may be empty to infer from `filename` (`.json` / `.csv`).
/// - `name` may be empty to derive a name from the filename stem.
///
/// Returns the table meta as JSON; throws the error message as a string on failure.
#[wasm_bindgen]
pub fn upload(name: &str, kind: &str, filename: &str, bytes: &[u8]) -> Result<String, JsValue> {
    let kind = if kind.is_empty() {
        infer_kind(filename).ok_or_else(|| err("could not determine kind; pass json or csv"))?
    } else {
        kind.to_string()
    };
    let name = if name.is_empty() {
        stem(filename)
    } else {
        name.to_string()
    };
    if name.is_empty() {
        return Err(err("missing table name"));
    }

    let source = saql_connectors::open_bytes(&kind, bytes).map_err(|e| err(&e.to_string()))?;
    let columns = schema_to_cols(&source.schema());
    let num_rows: usize = source
        .scan(None, &[])
        .map_err(|e| err(&e.to_string()))?
        .iter()
        .map(|b| b.num_rows())
        .sum();
    let meta = TableMeta {
        name: name.clone(),
        kind,
        columns,
        num_rows,
    };

    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.session.register_table(&name, source);
        s.tables.retain(|t| t.name != name); // re-uploading a name replaces it
        s.tables.push(meta.clone());
    });
    Ok(serde_json::to_string(&meta).unwrap())
}

/// Run a SQL query. Returns `{ columns, rows, num_rows, truncated }` as JSON;
/// throws the error message as a string on failure.
#[wasm_bindgen]
pub fn query(sql: &str) -> Result<String, JsValue> {
    let batches = STATE
        .with(|s| s.borrow().session.sql(sql))
        .map_err(|e| err(&e.to_string()))?;
    let resp = batches_to_resp(&batches).map_err(|e| err(&e))?;
    Ok(serde_json::to_string(&resp).unwrap())
}

/// Parse-only validation for the editor's live red-underline squiggles. Never
/// throws — returns `{ valid, message, line, col }` as JSON.
#[wasm_bindgen]
pub fn validate(sql: &str) -> String {
    let resp = match saql_core::frontend::parse(sql) {
        Ok(_) => ValidateResp {
            valid: true,
            message: None,
            line: None,
            col: None,
        },
        Err(e) => {
            let message = e.to_string();
            let (line, col) = extract_line_col(&message);
            ValidateResp {
                valid: false,
                message: Some(message),
                line,
                col,
            }
        }
    };
    serde_json::to_string(&resp).unwrap()
}

// ---- helpers ----------------------------------------------------------------

fn err(msg: &str) -> JsValue {
    JsValue::from_str(msg)
}

fn infer_kind(filename: &str) -> Option<String> {
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".json") {
        Some("json".into())
    } else if lower.ends_with(".csv") {
        Some("csv".into())
    } else {
        None
    }
}

fn stem(filename: &str) -> String {
    std::path::Path::new(filename)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn schema_to_cols(schema: &arrow::datatypes::Schema) -> Vec<ColumnMeta> {
    schema
        .fields()
        .iter()
        .map(|f| ColumnMeta {
            name: f.name().clone(),
            ty: f.data_type().to_string(),
        })
        .collect()
}

/// Render Arrow batches to a JSON-friendly column/row shape. Cells are formatted
/// to strings via Arrow's display kernel, so any column type renders uniformly.
fn batches_to_resp(batches: &[RecordBatch]) -> Result<QueryResp, String> {
    let columns = match batches.first() {
        Some(b) => schema_to_cols(&b.schema()),
        None => Vec::new(),
    };
    let total: usize = batches.iter().map(|b| b.num_rows()).sum();
    let opts = FormatOptions::default();
    let mut rows: Vec<Vec<String>> = Vec::new();

    'outer: for b in batches {
        let formatters: Vec<ArrayFormatter> = (0..b.num_columns())
            .map(|i| ArrayFormatter::try_new(b.column(i), &opts))
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| e.to_string())?;
        for r in 0..b.num_rows() {
            if rows.len() >= MAX_DISPLAY_ROWS {
                break 'outer;
            }
            rows.push(formatters.iter().map(|f| f.value(r).to_string()).collect());
        }
    }

    let truncated = total > rows.len();
    Ok(QueryResp {
        columns,
        rows,
        num_rows: total,
        truncated,
    })
}

/// Pull "Line: N" / "Column: N" out of a sqlparser error message.
fn extract_line_col(msg: &str) -> (Option<usize>, Option<usize>) {
    fn num_after(msg: &str, marker: &str) -> Option<usize> {
        let rest = &msg[msg.find(marker)? + marker.len()..];
        rest.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .ok()
    }
    (num_after(msg, "Line: "), num_after(msg, "Column: "))
}
