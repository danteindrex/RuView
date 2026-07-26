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
