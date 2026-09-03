use comfy_table::{Cell, CellAlignment, ContentArrangement, Table as PrettyTable};
use token_perf_core::store::Store;

pub enum Align {
    Left,
    Right,
}

pub fn apply_pretty(store: &Store, flag: Option<bool>) -> bool {
    if let Some(pretty) = flag {
        store
            .set_setting("pretty", if pretty { "on" } else { "off" })
            .ok();
        return pretty;
    }
    store.setting("pretty").ok().flatten().as_deref() == Some("on")
}

/// Renders one of the CLI's aligned columns-of-numbers listings.
///
/// `aligns` covers the fixed-width columns; when `trailing` is set, `headers`
/// and every row in `rows` carry one extra, unpadded free-text column at the
/// end (a title, or the tool list on a call).
pub fn print_table(
    pretty: bool,
    indent: &str,
    headers: &[&str],
    aligns: &[Align],
    rows: &[Vec<String>],
    trailing: bool,
) {
    if pretty {
        let mut table = PrettyTable::new();
        table
            .load_style(comfy_table::presets::UTF8_FULL_CONDENSED)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(headers.to_vec());
        for row in rows {
            let cells = row.iter().enumerate().map(|(i, v)| {
                let cell = Cell::new(v);
                match aligns.get(i) {
                    Some(Align::Right) => cell.set_alignment(CellAlignment::Right),
                    _ => cell,
                }
            });
            table.add_row(cells.collect::<Vec<_>>());
        }
        for line in table.to_string().lines() {
            println!("{indent}{line}");
        }
        return;
    }

    println!("{}", render_ascii(indent, headers, aligns, rows, trailing));
}

fn render_ascii(
    indent: &str,
    headers: &[&str],
    aligns: &[Align],
    rows: &[Vec<String>],
    trailing: bool,
) -> String {
    let widths: Vec<usize> = (0..aligns.len())
        .map(|i| {
            headers[i]
                .chars()
                .count()
                .max(rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(0))
        })
        .collect();
    let render_row = |cells: &[String]| {
        let mut line = String::new();
        for (i, w) in widths.iter().enumerate() {
            line.push_str(&match aligns[i] {
                Align::Left => format!("{:<w$} ", cells[i]),
                Align::Right => format!("{:>w$} ", cells[i]),
            });
        }
        if trailing {
            line.push(' ');
            line.push_str(&cells[aligns.len()]);
        }
        format!("{indent}{}", line.trim_end())
    };
    let header_cells: Vec<String> = headers.iter().map(|s| s.to_string()).collect();
    let mut lines = vec![render_row(&header_cells)];
    lines.extend(rows.iter().map(|r| render_row(r)));
    lines.join("\n")
}

#[cfg(test)]
#[path = "table_tests.rs"]
mod tests;
