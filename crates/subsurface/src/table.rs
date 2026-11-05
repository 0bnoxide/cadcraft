//! A plain table of cells: what spreadsheet reads return, and what reports and data
//! extraction write. Shared; change only by agreement (see the slice 6 brief).

use serde::{Deserialize, Serialize};

/// One cell value. Dates and times are kept as their text form.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Cell {
    #[default]
    Empty,
    Text(String),
    Number(f64),
    Bool(bool),
}

impl Cell {
    /// The cell as trimmed text; numbers use Rust's shortest round-trip form, `Empty` is "".
    pub fn text(&self) -> String {
        match self {
            Cell::Empty => String::new(),
            Cell::Text(s) => s.trim().to_string(),
            Cell::Number(n) => n.to_string(),
            Cell::Bool(b) => b.to_string(),
        }
    }

    /// The cell as a finite number, parsing text if needed.
    pub fn number(&self) -> Option<f64> {
        let n = match self {
            Cell::Number(n) => *n,
            Cell::Text(s) => s.trim().parse::<f64>().ok()?,
            _ => return None,
        };
        n.is_finite().then_some(n)
    }
}

/// A named sheet of rows. The first row is the header when a caller says so.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub rows: Vec<Vec<Cell>>,
}

impl Table {
    /// The first row as trimmed text, or an empty list.
    pub fn header(&self) -> Vec<String> {
        self.rows.first().map(|r| r.iter().map(Cell::text).collect()).unwrap_or_default()
    }

    /// Index of a header column, matched case-insensitively after trimming.
    pub fn column(&self, name: &str) -> Option<usize> {
        let want = name.trim().to_ascii_lowercase();
        self.header().iter().position(|h| h.to_ascii_lowercase() == want)
    }

    /// The cell at `row`, `col`, or `Empty` when out of range.
    pub fn cell(&self, row: usize, col: usize) -> &Cell {
        const EMPTY: &Cell = &Cell::Empty;
        self.rows.get(row).and_then(|r| r.get(col)).unwrap_or(EMPTY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_lookup_and_out_of_range_cells() {
        let t = Table {
            name: "s".into(),
            rows: vec![vec![Cell::Text(" Boring_ID ".into()), Cell::Text("Top_ft".into())], vec![Cell::Text("B-1".into()), Cell::Text("2.5".into())]],
        };
        assert_eq!(t.column("boring_id"), Some(0));
        assert_eq!(t.column("missing"), None);
        assert_eq!(t.cell(1, 1).number(), Some(2.5));
        assert_eq!(t.cell(9, 9), &Cell::Empty);
        assert_eq!(Cell::Text("NaN".into()).number(), None);
    }
}
