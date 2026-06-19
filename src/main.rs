//! SAQL editor — a small web server that links the SAQL engine.
//!
//! Endpoints:
//!   POST /api/upload  (multipart: file, optional name, optional kind) -> table meta
//!   POST /api/query   ({ "sql": "..." })                              -> { columns, rows }
//!   GET  /api/tables                                                  -> [table meta]
//!   GET  /*           static files from ./web (the UI)
//!
//! The engine runs in-process; uploaded files are decoded by saql-connectors
//! into in-memory Arrow tables held in a shared Session.

use std::sync::{Arc, Mutex};

use arrow::record_batch::RecordBatch;
use arrow::util::display::{ArrayFormatter, FormatOptions};
use axum::{
    extract::{DefaultBodyLimit, Multipart, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use saql_core::Session;
use serde::{Deserialize, Serialize};
use std::io::Write;
use tower_http::services::ServeDir;

/// Cap rows returned to the browser so a huge result can't wedge the DOM.
const MAX_DISPLAY_ROWS: usize = 5000;

struct AppState {
    session: Session,
    tables: Vec<TableMeta>,
}
type Shared = Arc<Mutex<AppState>>;

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

#[derive(Deserialize)]
struct QueryReq {
    sql: String,
}

#[derive(Serialize)]
struct ErrorResp {
    error: String,
}

/// A request error rendered as `{ "error": "..." }` with a status code.
struct AppError {
    status: StatusCode,
    message: String,
}

impl AppError {
    fn bad(msg: impl Into<String>) -> Self {
        AppError {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResp {
                error: self.message,
            }),
        )
            .into_response()
    }
}

impl From<saql_core::SaqlError> for AppError {
    fn from(e: saql_core::SaqlError) -> Self {
        AppError::bad(e.to_string())
    }
}

#[tokio::main]
async fn main() {
    let state: Shared = Arc::new(Mutex::new(AppState {
        session: Session::new(),
        tables: Vec::new(),
    }));

    let web_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/web");
    let app = Router::new()
        .route("/api/query", post(query))
        .route("/api/upload", post(upload))
        .route("/api/tables", get(list_tables))
        .fallback_service(ServeDir::new(web_dir))
        .layer(DefaultBodyLimit::max(256 * 1024 * 1024))
        .with_state(state);

    let addr = "127.0.0.1:7878";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("SAQL editor → http://{addr}");
    axum::serve(listener, app).await.unwrap();
}

async fn list_tables(State(state): State<Shared>) -> Json<Vec<TableMeta>> {
    let s = state.lock().unwrap();
    Json(s.tables.clone())
}

async fn query(
    State(state): State<Shared>,
    Json(req): Json<QueryReq>,
) -> Result<Json<QueryResp>, AppError> {
    let batches = {
        let s = state.lock().unwrap();
        s.session.sql(&req.sql)?
    };
    Ok(Json(batches_to_resp(&batches)?))
}

async fn upload(
    State(state): State<Shared>,
    mut multipart: Multipart,
) -> Result<Json<TableMeta>, AppError> {
    let mut name: Option<String> = None;
    let mut kind: Option<String> = None;
    let mut filename: Option<String> = None;
    let mut bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad(e.to_string()))?
    {
        match field.name() {
            Some("name") => name = Some(field.text().await.map_err(|e| AppError::bad(e.to_string()))?),
            Some("kind") => kind = Some(field.text().await.map_err(|e| AppError::bad(e.to_string()))?),
            Some("file") => {
                filename = field.file_name().map(|s| s.to_string());
                let data = field.bytes().await.map_err(|e| AppError::bad(e.to_string()))?;
                bytes = Some(data.to_vec());
            }
            _ => {}
        }
    }

    let bytes = bytes.ok_or_else(|| AppError::bad("missing `file` field"))?;
    let filename = filename.unwrap_or_default();
    let kind = kind
        .filter(|k| !k.is_empty())
        .or_else(|| infer_kind(&filename))
        .ok_or_else(|| AppError::bad("could not determine kind; pass json or csv"))?;
    let name = name
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| stem(&filename));
    if name.is_empty() {
        return Err(AppError::bad("missing table name"));
    }

    // The connector reads from a path, so stage the bytes in a temp file. It
    // loads eagerly into memory, so the temp file is gone after `open` returns.
    let mut tmp = tempfile::NamedTempFile::new().map_err(|e| AppError::bad(e.to_string()))?;
    tmp.write_all(&bytes).map_err(|e| AppError::bad(e.to_string()))?;
    tmp.flush().ok();
    let source = saql_connectors::open(&kind, &tmp.path().to_string_lossy())?;

    let columns = schema_to_cols(&source.schema());
    let num_rows: usize = source.scan(None, &[])?.iter().map(|b| b.num_rows()).sum();
    let meta = TableMeta {
        name: name.clone(),
        kind: kind.clone(),
        columns,
        num_rows,
    };

    {
        let mut s = state.lock().unwrap();
        s.session.register_table(&name, source);
        s.tables.retain(|t| t.name != name); // re-uploading a name replaces it
        s.tables.push(meta.clone());
    }
    Ok(Json(meta))
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
fn batches_to_resp(batches: &[RecordBatch]) -> Result<QueryResp, AppError> {
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
            .map_err(|e| AppError::bad(e.to_string()))?;
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
