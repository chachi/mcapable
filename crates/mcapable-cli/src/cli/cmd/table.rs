use tabled::builder::Builder;
use tabled::settings::{Margin, Padding, Style};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TableData {
    pub(crate) headers: Vec<String>,
    pub(crate) rows: Vec<Vec<String>>,
}

impl TableData {
    pub(crate) fn new(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self { headers, rows }
    }
}

pub(crate) fn render_table(table: &TableData) -> String {
    let mut builder = Builder::default();
    builder.push_record(table.headers.clone());
    for row in &table.rows {
        builder.push_record(row.clone());
    }

    let mut out = builder.build();
    out.with(Style::blank().vertical('\t'))
        .with(Padding::zero())
        .with(Margin::new(0, 0, 0, 0));
    let rendered = out.to_string();
    if rendered.is_empty() {
        return rendered;
    }
    rendered
        .lines()
        .map(|line| format!("{line}\t"))
        .collect::<Vec<_>>()
        .join("\n")
}
