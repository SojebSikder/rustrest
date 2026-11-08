//! Code folding for the pretty-printed JSON response body.

use std::collections::{BTreeMap, BTreeSet};

/// placeholder shown in place of a collapsed region's contents
const ELLIPSIS: &str = " … ";

#[derive(Debug, Clone, Default)]
pub struct JsonFold {
    lines: Vec<String>,
    /// foldable regions: opening line -> closing line (full-text line numbers)
    regions: BTreeMap<usize, usize>,
    /// opening lines of the collapsed regions
    collapsed: BTreeSet<usize>,
}

/// one line of the folded view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayLine {
    /// line number in the full text
    pub line: usize,
    /// whether a region starts here (so it gets a fold toggle)
    pub foldable: bool,
    pub collapsed: bool,
}

impl JsonFold {
    /// `foldable` turns region detection off for text that isn't JSON, where
    /// brackets inside prose or markup would produce nonsense regions.
    pub fn new(text: &str, foldable: bool) -> Self {
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        let regions = if foldable {
            find_regions(&lines)
        } else {
            BTreeMap::new()
        };
        Self {
            lines,
            regions,
            collapsed: BTreeSet::new(),
        }
    }

    pub fn has_regions(&self) -> bool {
        !self.regions.is_empty()
    }

    pub fn full_text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn toggle(&mut self, line: usize) {
        if self.regions.contains_key(&line) && !self.collapsed.remove(&line) {
            self.collapsed.insert(line);
        }
    }

    pub fn expand_all(&mut self) {
        self.collapsed.clear();
    }

    /// collapses every region except the outermost ones, so the top-level
    /// keys stay visible (expanding one then reveals its children collapsed).
    pub fn collapse_all(&mut self) {
        let mut outer_end = None;
        self.collapsed.clear();
        for (&start, &end) in &self.regions {
            match outer_end {
                Some(e) if start < e => {
                    self.collapsed.insert(start);
                }
                _ => outer_end = Some(end),
            }
        }
    }

    /// the visible lines, top to bottom, with their full-text line numbers
    pub fn display_lines(&self) -> Vec<DisplayLine> {
        let mut out = Vec::with_capacity(self.lines.len());
        let mut i = 0;

        while i < self.lines.len() {
            let region_end = self.regions.get(&i).copied();
            let collapsed = region_end.is_some() && self.collapsed.contains(&i);
            out.push(DisplayLine {
                line: i,
                foldable: region_end.is_some(),
                collapsed,
            });
            i = match region_end {
                Some(end) if collapsed => end + 1,
                _ => i + 1,
            };
        }
        out
    }

    /// the text to show in the editor, collapsed regions shortened
    pub fn display_text(&self) -> String {
        self.rows_text(&self.display_lines())
    }

    /// the editor text of one visible line
    pub fn row_text(&self, dl: &DisplayLine) -> String {
        let mut out = self.lines[dl.line].clone();
        if dl.collapsed {
            let end = self.regions[&dl.line];
            out.push_str(ELLIPSIS);
            out.push_str(self.lines[end].trim_start());
        }
        out
    }

    /// visible lines joined into editor text
    pub fn rows_text(&self, rows: &[DisplayLine]) -> String {
        rows.iter()
            .map(|dl| self.row_text(dl))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// matches brackets across lines (ignoring ones inside strings) and records
/// every pair that spans more than one line.
fn find_regions(lines: &[String]) -> BTreeMap<usize, usize> {
    let mut regions = BTreeMap::new();
    let mut stack: Vec<usize> = Vec::new();

    for (line_no, line) in lines.iter().enumerate() {
        let mut in_string = false;
        let mut escaped = false;

        for c in line.chars() {
            if in_string {
                match c {
                    _ if escaped => escaped = false,
                    '\\' => escaped = true,
                    '"' => in_string = false,
                    _ => {}
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '{' | '[' => stack.push(line_no),
                '}' | ']' => {
                    if let Some(start) = stack.pop()
                        && line_no > start
                    {
                        // a line can open several brackets. inner ones close
                        // first, so overwriting leaves the outermost region
                        regions.insert(start, line_no);
                    }
                }
                _ => {}
            }
        }
    }
    regions
}
