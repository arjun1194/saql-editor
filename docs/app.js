// SAQL editor — static / WebAssembly build.
//
// Identical UI to the server version, but the four data operations call the
// SAQL engine compiled to WASM (window.SAQL, set up by the module script in
// index.html) instead of hitting /api/* endpoints. Everything runs in the page.

const $ = (id) => document.getElementById(id);

let editor = null;
let catalog = []; // [{ name, kind, columns:[{name,type}], num_rows }]

const KEYWORDS = ["SELECT", "FROM", "WHERE", "AND", "OR", "AS", "LIMIT", "NOT", "NULL", "TRUE", "FALSE"];

function esc(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
}
function setStatus(msg, isError = false) {
  const s = $("status");
  s.textContent = msg;
  s.className = "status" + (isError ? " err" : "");
}
function quoteIfNeeded(name) {
  return /[^A-Za-z0-9_]/.test(name) ? `"${name}"` : name;
}
// Errors thrown across the wasm boundary arrive as plain strings.
function errText(e) {
  return e && e.message ? e.message : String(e);
}

// ---- Tables / catalog ------------------------------------------------------

async function loadTables() {
  await window.saqlReady;
  catalog = JSON.parse(window.SAQL.tables());
  renderTableList();
}

function renderTableList() {
  const el = $("tableList");
  if (!catalog.length) {
    el.innerHTML = '<p class="empty">No tables yet.</p>';
    return;
  }
  el.innerHTML = "";
  for (const t of catalog) {
    const div = document.createElement("div");
    div.className = "table-item";
    const cols = t.columns
      .map((c) => `<div class="app-col"><span class="cname">${esc(c.name)}</span><span class="ctype">${esc(c.type)}</span></div>`)
      .join("");
    div.innerHTML =
      `<div class="thead"><span class="tn">${esc(t.name)}</span>` +
      `<span class="tmeta">${esc(t.kind)} · ${t.num_rows} rows</span></div>` +
      `<div class="app-cols">${cols}</div>`;
    div.querySelector(".thead").onclick = () => {
      if (!editor) return;
      editor.setValue(`SELECT * FROM ${t.name} LIMIT 100`);
      editor.focus();
      runQuery();
    };
    el.appendChild(div);
  }
}

async function uploadFile() {
  const f = $("file").files[0];
  if (!f) {
    setStatus("choose a file first", true);
    return;
  }
  const name = $("tname").value.trim();
  const kind = $("kind").value;

  setStatus("loading…");
  try {
    await window.saqlReady;
    const bytes = new Uint8Array(await f.arrayBuffer());
    const meta = JSON.parse(window.SAQL.upload(name, kind, f.name, bytes));
    setStatus(`attached "${meta.name}" — ${meta.num_rows} rows`);
    $("tname").value = "";
    $("file").value = "";
    await loadTables();
  } catch (e) {
    setStatus(errText(e), true);
  }
}

// ---- Query -----------------------------------------------------------------

async function runQuery() {
  const sql = (editor ? editor.getValue() : "").trim();
  if (!sql) return;
  setStatus("running…");
  const t0 = performance.now();
  try {
    await window.saqlReady;
    const data = JSON.parse(window.SAQL.query(sql));
    const ms = (performance.now() - t0).toFixed(0);
    renderResults(data);
    const shown = data.truncated ? ` (showing first ${data.rows.length})` : "";
    setStatus(`${data.num_rows} rows · ${ms} ms${shown}`);
  } catch (e) {
    renderError(errText(e));
    setStatus(errText(e), true);
  }
}

function renderResults(data) {
  const el = $("results");
  if (!data.columns.length) {
    el.innerHTML = '<p class="empty">(no columns)</p>';
    return;
  }
  const head = data.columns
    .map((c) => `<th>${esc(c.name)}<span class="th-type">${esc(c.type)}</span></th>`)
    .join("");
  const body = data.rows
    .map((row) => `<tr>${row.map((v) => `<td>${esc(v)}</td>`).join("")}</tr>`)
    .join("");
  el.innerHTML = `<table><thead><tr>${head}</tr></thead><tbody>${body}</tbody></table>`;
}
function renderError(msg) {
  $("results").innerHTML = `<pre class="error">${esc(msg || "error")}</pre>`;
}

// ---- Monaco: editor, schema autocomplete, live validation ------------------

require.config({ paths: { vs: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs" } });
require(["vs/editor/editor.main"], () => {
  editor = monaco.editor.create($("editor"), {
    value:
      "-- Upload a JSON/CSV file, then query it. Examples:\n" +
      "--   SELECT * FROM customers LIMIT 100\n" +
      "--   SELECT \"First Name\", City FROM customers WHERE Country = 'United States'\n",
    language: "sql",
    theme: "vs",
    minimap: { enabled: false },
    fontSize: 13.5,
    automaticLayout: true,
    scrollBeyondLastLine: false,
    padding: { top: 8 },
    suggestSelection: "first",
  });

  registerCompletion();
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, () => runQuery());

  let timer;
  editor.onDidChangeModelContent(() => {
    clearTimeout(timer);
    timer = setTimeout(validateNow, 250);
  });
});

function registerCompletion() {
  monaco.languages.registerCompletionItemProvider("sql", {
    triggerCharacters: [" ", '"'],
    provideCompletionItems(model, position) {
      const word = model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      };
      const before = model.getValueInRange({
        startLineNumber: 1,
        startColumn: 1,
        endLineNumber: position.lineNumber,
        endColumn: position.column,
      });
      const afterFrom = /\bfrom\s+["\w]*$/i.test(before);
      const K = monaco.languages.CompletionItemKind;
      const items = [];

      for (const t of catalog) {
        items.push({
          label: t.name,
          kind: K.Struct,
          insertText: quoteIfNeeded(t.name),
          range,
          detail: `table · ${t.num_rows} rows`,
          sortText: (afterFrom ? "0" : "2") + t.name,
        });
      }
      if (!afterFrom) {
        for (const t of catalog) {
          for (const c of t.columns) {
            items.push({
              label: c.name,
              kind: K.Field,
              insertText: quoteIfNeeded(c.name),
              range,
              detail: `${c.type} · ${t.name}`,
              sortText: "1" + c.name,
            });
          }
        }
        for (const kw of KEYWORDS) {
          items.push({ label: kw, kind: K.Keyword, insertText: kw, range, sortText: "3" + kw });
        }
      }
      return { suggestions: items };
    },
  });
}

function cleanMessage(m) {
  return (m || "")
    .replace(/^parse error:\s*/, "")
    .replace(/^sql parser error:\s*/, "")
    .replace(/ at Line: \d+, Column: \d+/, "");
}

async function validateNow() {
  if (!editor) return;
  const model = editor.getModel();
  const sql = editor.getValue();
  if (!sql.trim()) {
    monaco.editor.setModelMarkers(model, "saql", []);
    return;
  }
  try {
    await window.saqlReady;
    const data = JSON.parse(window.SAQL.validate(sql));
    if (data.valid) {
      monaco.editor.setModelMarkers(model, "saql", []);
      return;
    }
    const line = data.line || 1;
    const col = data.col || 1;
    monaco.editor.setModelMarkers(model, "saql", [
      {
        severity: monaco.MarkerSeverity.Error,
        message: cleanMessage(data.message),
        startLineNumber: line,
        startColumn: col,
        endLineNumber: line,
        endColumn: col + 1,
      },
    ]);
  } catch (e) {
    /* validation is best-effort */
  }
}

$("uploadBtn").onclick = uploadFile;
$("runBtn").onclick = () => runQuery();

// Show progress while the (~5 MB) engine wasm downloads + initializes.
setStatus("loading the SAQL engine…");
window.saqlReady.then(() => setStatus("engine ready — upload a file to begin"));
loadTables();
