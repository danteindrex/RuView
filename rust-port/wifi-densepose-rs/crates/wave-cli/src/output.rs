//! Output engine — TTY-aware rendering (clig.dev: human-readable to a terminal,
//! machine-readable when piped). Primary results go to stdout; logs/diagnostics
//! to stderr (handled by callers).

use std::io::IsTerminal;

use clap::ValueEnum;

/// Output format, chosen with the global `-o/--output` flag. `Auto` renders a
/// table on a TTY and JSON when piped.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Auto,
    Table,
    Json,
    Yaml,
}

impl Format {
    /// Resolve `Auto` against whether stdout is a terminal.
    fn resolved(self) -> Format {
        match self {
            Format::Auto => {
                if std::io::stdout().is_terminal() {
                    Format::Table
                } else {
                    Format::Json
                }
            }
            other => other,
        }
    }
}

/// Whether color should be emitted: off when piped, when `NO_COLOR` is set, or
/// when `--no-color` was passed.
pub fn use_color(no_color_flag: bool) -> bool {
    if no_color_flag || std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::io::stdout().is_terminal()
}

/// A simple tabular view: header + rows of strings. Used for the `table` format;
/// `json`/`yaml` render the underlying value instead.
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(headers: &[&str]) -> Self {
        Table {
            headers: headers.iter().map(|s| s.to_string()).collect(),
            rows: Vec::new(),
        }
    }

    pub fn row(&mut self, cells: Vec<String>) {
        self.rows.push(cells);
    }

    fn render(&self) -> String {
        use comfy_table::{presets::UTF8_FULL, ContentArrangement, Table as CTable};
        let mut t = CTable::new();
        t.load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(self.headers.clone());
        for r in &self.rows {
            t.add_row(r.clone());
        }
        t.to_string()
    }
}

/// Render a value in the resolved format. `table` is only used for `Format::Table`;
/// `json`/`yaml` serialize `value`. Returns the text to print to stdout.
pub fn render<T: serde::Serialize>(
    fmt: Format,
    value: &T,
    table: impl FnOnce() -> Table,
) -> anyhow::Result<String> {
    Ok(match fmt.resolved() {
        Format::Table => table().render(),
        Format::Json => serde_json::to_string_pretty(value)?,
        Format::Yaml => serde_yaml::to_string(value)?,
        Format::Auto => unreachable!("resolved() removes Auto"),
    })
}

/// Print a success/status line to stderr (so it never pollutes piped stdout data).
pub fn note(msg: &str) {
    eprintln!("{msg}");
}

/// Render an arbitrary JSON value in the resolved format, auto-shaping tables:
/// an object → 2-col key/value; an array of objects (or `{items|nodes|models|
/// recordings:[...]}`) → columns from the union of keys; anything else → text.
/// Lets thin REST commands be one-liners.
pub fn auto(fmt: Format, value: &serde_json::Value) -> anyhow::Result<String> {
    Ok(match fmt.resolved() {
        Format::Json => serde_json::to_string_pretty(value)?,
        Format::Yaml => serde_yaml::to_string(value)?,
        Format::Table => value_to_table(value),
        Format::Auto => unreachable!("resolved() removes Auto"),
    })
}

fn value_to_table(v: &serde_json::Value) -> String {
    // Unwrap a common single-array wrapper key.
    let arr = v.as_array().cloned().or_else(|| {
        for k in ["items", "nodes", "models", "recordings", "profiles", "results", "data"] {
            if let Some(a) = v.get(k).and_then(|x| x.as_array()) {
                return Some(a.clone());
            }
        }
        None
    });

    if let Some(rows) = arr {
        if rows.is_empty() {
            return "(empty)".to_string();
        }
        // Column union across rows (stable order from the first row, then extras).
        let mut cols: Vec<String> = Vec::new();
        for r in &rows {
            if let Some(o) = r.as_object() {
                for k in o.keys() {
                    if !cols.contains(k) {
                        cols.push(k.clone());
                    }
                }
            }
        }
        if cols.is_empty() {
            // Array of scalars.
            let mut t = Table::new(&["VALUE"]);
            for r in &rows {
                t.row(vec![scalar(r)]);
            }
            return t.render();
        }
        let headers: Vec<&str> = cols.iter().map(|s| s.as_str()).collect();
        let mut t = Table::new(&headers);
        for r in &rows {
            t.row(cols.iter().map(|c| r.get(c).map(scalar).unwrap_or_default()).collect());
        }
        return t.render();
    }

    if let Some(obj) = v.as_object() {
        let mut t = Table::new(&["FIELD", "VALUE"]);
        for (k, val) in obj {
            t.row(vec![k.clone(), scalar(val)]);
        }
        return t.render();
    }

    scalar(v)
}

/// Compact one-cell string for a JSON value (unquote strings; short-render nested).
fn scalar(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => "-".to_string(),
        other => {
            let s = other.to_string();
            if s.len() > 60 {
                format!("{}…", &s[..59])
            } else {
                s
            }
        }
    }
}
