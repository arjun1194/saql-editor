const $ = (id) => document.getElementById(id);

function esc(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
}

function setStatus(msg, isError = false) {
  const s = $("status");
  s.textContent = msg;
  s.className = "status" + (isError ? " err" : "");
}

async function loadTables() {
  const res = await fetch("/api/tables");
  const tables = await res.json();
  const el = $("tableList");
  if (!tables.length) {
    el.innerHTML = '<p class="empty">No tables yet.</p>';
    return;
  }
  el.innerHTML = "";
  for (const t of tables) {
    const div = document.createElement("div");
    div.className = "table-item";
    const cols = t.columns
      .map((c) => `<div class="col"><span class="cname">${esc(c.name)}</span><span class="ctype">${esc(c.type)}</span></div>`)
      .join("");
    div.innerHTML =
      `<div class="thead"><span class="tn">${esc(t.name)}</span>` +
      `<span class="tmeta">${esc(t.kind)} · ${t.num_rows} rows</span></div>` +
      `<div class="cols">${cols}</div>`;
    div.querySelector(".thead").onclick = () => {
      $("sql").value = `SELECT * FROM ${t.name} LIMIT 100`;
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
  const fd = new FormData();
  fd.append("file", f);
  const name = $("tname").value.trim();
  if (name) fd.append("name", name);
  if ($("kind").value) fd.append("kind", $("kind").value);

  setStatus("uploading…");
  try {
    const res = await fetch("/api/upload", { method: "POST", body: fd });
    const data = await res.json();
    if (!res.ok) {
      setStatus(data.error || "upload failed", true);
      return;
    }
    setStatus(`attached "${data.name}" — ${data.num_rows} rows`);
    $("tname").value = "";
    $("file").value = "";
    await loadTables();
  } catch (e) {
    setStatus(String(e), true);
  }
}

async function runQuery() {
  const sql = $("sql").value.trim();
  if (!sql) return;
  setStatus("running…");
  const t0 = performance.now();
  try {
    const res = await fetch("/api/query", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ sql }),
    });
    const data = await res.json();
    const ms = (performance.now() - t0).toFixed(0);
    if (!res.ok) {
      renderError(data.error || "query failed");
      setStatus(data.error || "query failed", true);
      return;
    }
    renderResults(data);
    const shown = data.truncated ? ` (showing first ${data.rows.length})` : "";
    setStatus(`${data.num_rows} rows · ${ms} ms${shown}`);
  } catch (e) {
    renderError(String(e));
    setStatus(String(e), true);
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

$("uploadBtn").onclick = uploadFile;
$("runBtn").onclick = runQuery;
$("sql").addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
    e.preventDefault();
    runQuery();
  }
});

loadTables();
