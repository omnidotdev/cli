//! Diff widget for displaying file changes in collapsed/expanded view.

use std::fmt;

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

/// Threshold width for switching between split and unified diff views.
/// Width >= `SPLIT_THRESHOLD` uses split view, otherwise unified view.
#[allow(dead_code)]
pub const SPLIT_THRESHOLD: u16 = 100;

/// Color for added lines (green)
pub const DIFF_ADD: Color = Color::Rgb(80, 160, 80);
/// Color for deleted lines (red)
pub const DIFF_DEL: Color = Color::Rgb(180, 80, 80);
/// Color for hunk headers (blue)
pub const DIFF_HUNK: Color = Color::Rgb(80, 140, 180);
/// Color for line numbers in gutter (dimmed)
const DIMMED: Color = Color::Rgb(100, 100, 110);

/// Tag indicating the type of diff line
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DiffTag {
    /// Line added in new version
    Add,
    /// Line removed from old version
    Delete,
    /// Line unchanged between versions
    Equal,
    /// Hunk header (e.g., @@ -1,3 +1,4 @@)
    Header,
}

/// A single line in a diff with metadata
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DiffLine {
    /// Type of change
    pub tag: DiffTag,
    /// Line content (without leading +/- marker)
    pub content: String,
    /// Line number in old file (None for added lines)
    pub old_line_num: Option<u32>,
    /// Line number in new file (None for deleted lines)
    pub new_line_num: Option<u32>,
}

/// Parsed diff with file path and structured lines
#[derive(Debug)]
#[allow(dead_code)]
pub struct ParsedDiff {
    /// File path extracted from --- / +++ headers
    pub file_path: Option<String>,
    /// Structured diff lines
    pub lines: Vec<DiffLine>,
}

/// Diff view widget with collapsible state
#[allow(dead_code)]
pub struct DiffView {
    /// Parsed diff data
    pub diff: ParsedDiff,
    /// Whether the diff is expanded
    pub is_expanded: bool,
}

impl DiffView {
    /// Create a new diff view from a diff string
    #[allow(dead_code)]
    pub fn new(diff_string: &str) -> Self {
        Self {
            diff: parse_diff(diff_string),
            is_expanded: false,
        }
    }

    /// Toggle the expanded state
    #[allow(dead_code)]
    pub const fn toggle(&mut self) {
        self.is_expanded = !self.is_expanded;
    }
}

impl fmt::Debug for DiffView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DiffView")
            .field("file_path", &self.diff.file_path)
            .field("line_count", &self.diff.lines.len())
            .field("is_expanded", &self.is_expanded)
            .finish()
    }
}

/// Parse a unified diff string into structured data
#[allow(dead_code)]
fn parse_diff(diff_str: &str) -> ParsedDiff {
    let mut file_path: Option<String> = None;
    let mut lines = Vec::new();
    let mut old_line_num: u32 = 0;
    let mut new_line_num: u32 = 0;

    for line in diff_str.lines() {
        // Extract file path from --- / +++ headers
        if line.starts_with("--- ") {
            let path = line.strip_prefix("--- ").unwrap_or("");
            if path != "/dev/null" {
                file_path = Some(path.to_string());
            }
            continue;
        }
        if line.starts_with("+++ ") {
            let path = line.strip_prefix("+++ ").unwrap_or("");
            if path != "/dev/null" && file_path.is_none() {
                file_path = Some(path.to_string());
            }
            continue;
        }

        // Parse hunk header (@@ -old,count +new,count @@)
        if line.starts_with("@@") {
            if let Some(header_content) = parse_hunk_header(line) {
                old_line_num = header_content.old_start;
                new_line_num = header_content.new_start;
                lines.push(DiffLine {
                    tag: DiffTag::Header,
                    content: line.to_string(),
                    old_line_num: None,
                    new_line_num: None,
                });
            }
            continue;
        }

        // Parse diff lines
        if let Some(stripped) = line.strip_prefix('+') {
            lines.push(DiffLine {
                tag: DiffTag::Add,
                content: stripped.to_string(),
                old_line_num: None,
                new_line_num: Some(new_line_num),
            });
            new_line_num += 1;
        } else if let Some(stripped) = line.strip_prefix('-') {
            lines.push(DiffLine {
                tag: DiffTag::Delete,
                content: stripped.to_string(),
                old_line_num: Some(old_line_num),
                new_line_num: None,
            });
            old_line_num += 1;
        } else if let Some(stripped) = line.strip_prefix(' ') {
            lines.push(DiffLine {
                tag: DiffTag::Equal,
                content: stripped.to_string(),
                old_line_num: Some(old_line_num),
                new_line_num: Some(new_line_num),
            });
            old_line_num += 1;
            new_line_num += 1;
        }
    }

    ParsedDiff { file_path, lines }
}

/// Hunk header metadata
#[allow(dead_code)]
struct HunkHeader {
    old_start: u32,
    new_start: u32,
}

/// Parse hunk header line to extract line numbers
#[allow(dead_code)]
fn parse_hunk_header(line: &str) -> Option<HunkHeader> {
    // Format: @@ -old_start,old_count +new_start,new_count @@
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }

    let old_part = parts[1].strip_prefix('-')?;
    let new_part = parts[2].strip_prefix('+')?;

    let old_start = old_part.split(',').next()?.parse::<u32>().ok()?;
    let new_start = new_part.split(',').next()?.parse::<u32>().ok()?;

    Some(HunkHeader {
        old_start,
        new_start,
    })
}

/// Render a diff line with line numbers in unified format
#[allow(dead_code)]
pub fn render_unified_line(line: &DiffLine) -> Line<'static> {
    let old_num = line
        .old_line_num
        .map_or("    ".to_string(), |n| format!("{n:>4}"));
    let new_num = line
        .new_line_num
        .map_or("    ".to_string(), |n| format!("{n:>4}"));

    let (prefix, content_style) = match line.tag {
        DiffTag::Add => ("+", Style::default().fg(DIFF_ADD)),
        DiffTag::Delete => ("-", Style::default().fg(DIFF_DEL)),
        DiffTag::Equal => (" ", Style::default()),
        DiffTag::Header => ("", Style::default().fg(DIFF_HUNK)),
    };

    let gutter_style = Style::default().fg(DIMMED);

    if line.tag == DiffTag::Header {
        Line::from(vec![Span::styled(line.content.clone(), content_style)])
    } else {
        Line::from(vec![
            Span::styled(old_num, gutter_style),
            Span::raw(" "),
            Span::styled(new_num, gutter_style),
            Span::raw(" | "),
            Span::styled(prefix, content_style),
            Span::styled(line.content.clone(), content_style),
        ])
    }
}

/// Render a diff line with line numbers in split format
#[allow(dead_code)]
pub fn render_split_line(line: &DiffLine, side: SplitSide) -> Line<'static> {
    let (line_num, content_style) = match (side, line.tag) {
        (SplitSide::Left, DiffTag::Delete) => (line.old_line_num, Style::default().fg(DIFF_DEL)),
        (SplitSide::Left, DiffTag::Equal) => (line.old_line_num, Style::default()),
        (SplitSide::Right, DiffTag::Add) => (line.new_line_num, Style::default().fg(DIFF_ADD)),
        (SplitSide::Right, DiffTag::Equal) => (line.new_line_num, Style::default()),
        _ => (None, Style::default()),
    };

    let num_str = line_num.map_or("    ".to_string(), |n| format!("{n:>4}"));
    let gutter_style = Style::default().fg(DIMMED);

    let prefix = match line.tag {
        DiffTag::Add => "+",
        DiffTag::Delete => "-",
        DiffTag::Equal => " ",
        DiffTag::Header => "",
    };

    if line.tag == DiffTag::Header {
        Line::from(vec![Span::styled(
            line.content.clone(),
            Style::default().fg(DIFF_HUNK),
        )])
    } else {
        Line::from(vec![
            Span::styled(num_str, gutter_style),
            Span::raw(" | "),
            Span::styled(prefix, content_style),
            Span::styled(line.content.clone(), content_style),
        ])
    }
}

/// Side indicator for split view rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SplitSide {
    Left,
    Right,
}

/// Render a parsed diff as styled lines for display.
///
/// Selects layout based on width:
/// - `width >= SPLIT_THRESHOLD`: Split view (old left, new right)
/// - `width < SPLIT_THRESHOLD`: Unified view (single column with +/- prefixes)
#[allow(dead_code)]
pub fn render_diff(diff: &ParsedDiff, width: u16) -> Vec<Line<'static>> {
    if width >= SPLIT_THRESHOLD {
        render_split_view(diff, width)
    } else {
        render_unified_view(diff)
    }
}

fn render_unified_view(diff: &ParsedDiff) -> Vec<Line<'static>> {
    diff.lines
        .iter()
        .map(|line| {
            let (prefix, style) = match line.tag {
                DiffTag::Add => ("+", Style::default().fg(DIFF_ADD)),
                DiffTag::Delete => ("-", Style::default().fg(DIFF_DEL)),
                DiffTag::Equal => (" ", Style::default()),
                DiffTag::Header => ("", Style::default().fg(DIFF_HUNK)),
            };

            let content = if line.tag == DiffTag::Header {
                line.content.clone()
            } else {
                format!("{prefix}{}", line.content)
            };

            Line::from(Span::styled(content, style))
        })
        .collect()
}

fn render_split_view(diff: &ParsedDiff, width: u16) -> Vec<Line<'static>> {
    let col_width = (width.saturating_sub(3) / 2) as usize;
    let mut lines = Vec::new();

    let mut i = 0;
    while i < diff.lines.len() {
        let line = &diff.lines[i];

        match line.tag {
            DiffTag::Header => {
                lines.push(Line::from(Span::styled(
                    line.content.clone(),
                    Style::default().fg(DIFF_HUNK),
                )));
                i += 1;
            }
            DiffTag::Equal => {
                let left = truncate_or_pad(&line.content, col_width);
                let right = truncate_or_pad(&line.content, col_width);
                lines.push(Line::from(vec![
                    Span::raw(left),
                    Span::raw(" | "),
                    Span::raw(right),
                ]));
                i += 1;
            }
            DiffTag::Delete => {
                let next = diff.lines.get(i + 1);
                if let Some(next_line) = next {
                    if next_line.tag == DiffTag::Add {
                        let left = truncate_or_pad(&line.content, col_width);
                        let right = truncate_or_pad(&next_line.content, col_width);
                        lines.push(Line::from(vec![
                            Span::styled(left, Style::default().fg(DIFF_DEL)),
                            Span::raw(" | "),
                            Span::styled(right, Style::default().fg(DIFF_ADD)),
                        ]));
                        i += 2;
                        continue;
                    }
                }
                let left = truncate_or_pad(&line.content, col_width);
                let right = " ".repeat(col_width);
                lines.push(Line::from(vec![
                    Span::styled(left, Style::default().fg(DIFF_DEL)),
                    Span::raw(" | "),
                    Span::raw(right),
                ]));
                i += 1;
            }
            DiffTag::Add => {
                let left = " ".repeat(col_width);
                let right = truncate_or_pad(&line.content, col_width);
                lines.push(Line::from(vec![
                    Span::raw(left),
                    Span::raw(" | "),
                    Span::styled(right, Style::default().fg(DIFF_ADD)),
                ]));
                i += 1;
            }
        }
    }

    lines
}

fn truncate_or_pad(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len > width {
        s.chars().take(width.saturating_sub(1)).collect::<String>() + "…"
    } else {
        format!("{s:<width$}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_diff() {
        let diff = r#"--- src/main.rs
+++ src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
-    println!("old");
+    println!("new");
+    println!("added");
 }
"#;

        let parsed = parse_diff(diff);
        assert_eq!(parsed.file_path, Some("src/main.rs".to_string()));
        assert_eq!(parsed.lines.len(), 6);

        assert_eq!(parsed.lines[0].tag, DiffTag::Header);

        assert_eq!(parsed.lines[1].tag, DiffTag::Equal);
        assert_eq!(parsed.lines[1].content, "fn main() {");
        assert_eq!(parsed.lines[1].old_line_num, Some(1));
        assert_eq!(parsed.lines[1].new_line_num, Some(1));

        assert_eq!(parsed.lines[2].tag, DiffTag::Delete);
        assert_eq!(parsed.lines[2].content, "    println!(\"old\");");
        assert_eq!(parsed.lines[2].old_line_num, Some(2));
        assert_eq!(parsed.lines[2].new_line_num, None);

        assert_eq!(parsed.lines[3].tag, DiffTag::Add);
        assert_eq!(parsed.lines[3].content, "    println!(\"new\");");
        assert_eq!(parsed.lines[3].old_line_num, None);
        assert_eq!(parsed.lines[3].new_line_num, Some(2));

        assert_eq!(parsed.lines[4].tag, DiffTag::Add);
        assert_eq!(parsed.lines[4].content, "    println!(\"added\");");
        assert_eq!(parsed.lines[4].new_line_num, Some(3));
    }

    #[test]
    fn test_toggle_state() {
        let diff = "--- test.txt\n+++ test.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let mut view = DiffView::new(diff);

        assert!(!view.is_expanded);
        view.toggle();
        assert!(view.is_expanded);
        view.toggle();
        assert!(!view.is_expanded);
    }

    #[test]
    fn test_file_path_extraction() {
        let diff = "--- src/lib.rs\n+++ src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
        let parsed = parse_diff(diff);
        assert_eq!(parsed.file_path, Some("src/lib.rs".to_string()));
    }

    #[test]
    fn test_file_path_from_new_file() {
        let diff = "--- /dev/null\n+++ src/new.rs\n@@ -0,0 +1 @@\n+new file\n";
        let parsed = parse_diff(diff);
        assert_eq!(parsed.file_path, Some("src/new.rs".to_string()));
    }

    #[test]
    fn test_line_number_tracking() {
        let diff = r"--- test.txt
+++ test.txt
@@ -10,4 +10,5 @@
 line 10
-line 11
+line 11 modified
 line 12
+line 13 added
 line 14
";

        let parsed = parse_diff(diff);

        let equal_line = parsed
            .lines
            .iter()
            .find(|l| l.tag == DiffTag::Equal)
            .unwrap();
        assert_eq!(equal_line.old_line_num, Some(10));
        assert_eq!(equal_line.new_line_num, Some(10));

        let delete_line = parsed
            .lines
            .iter()
            .find(|l| l.tag == DiffTag::Delete)
            .unwrap();
        assert_eq!(delete_line.old_line_num, Some(11));
        assert_eq!(delete_line.new_line_num, None);

        let add_lines: Vec<_> = parsed
            .lines
            .iter()
            .filter(|l| l.tag == DiffTag::Add)
            .collect();
        assert_eq!(add_lines[0].new_line_num, Some(11));
        assert_eq!(add_lines[1].new_line_num, Some(13));
    }

    #[test]
    fn test_unified_line_rendering() {
        let equal_line = DiffLine {
            tag: DiffTag::Equal,
            content: "fn main() {".to_string(),
            old_line_num: Some(1),
            new_line_num: Some(1),
        };
        let rendered = render_unified_line(&equal_line);
        assert_eq!(rendered.spans.len(), 6);
        assert_eq!(rendered.spans[0].content, "   1");
        assert_eq!(rendered.spans[2].content, "   1");
        assert_eq!(rendered.spans[4].content, " ");
        assert_eq!(rendered.spans[5].content, "fn main() {");

        let delete_line = DiffLine {
            tag: DiffTag::Delete,
            content: "    old line".to_string(),
            old_line_num: Some(2),
            new_line_num: None,
        };
        let rendered = render_unified_line(&delete_line);
        assert_eq!(rendered.spans[0].content, "   2");
        assert_eq!(rendered.spans[2].content, "    ");
        assert_eq!(rendered.spans[4].content, "-");

        let add_line = DiffLine {
            tag: DiffTag::Add,
            content: "    new line".to_string(),
            old_line_num: None,
            new_line_num: Some(2),
        };
        let rendered = render_unified_line(&add_line);
        assert_eq!(rendered.spans[0].content, "    ");
        assert_eq!(rendered.spans[2].content, "   2");
        assert_eq!(rendered.spans[4].content, "+");
    }

    #[test]
    fn test_split_line_rendering() {
        let delete_line = DiffLine {
            tag: DiffTag::Delete,
            content: "old content".to_string(),
            old_line_num: Some(10),
            new_line_num: None,
        };
        let left_rendered = render_split_line(&delete_line, SplitSide::Left);
        assert_eq!(left_rendered.spans[0].content, "  10");
        assert_eq!(left_rendered.spans[2].content, "-");
        assert_eq!(left_rendered.spans[3].content, "old content");

        let add_line = DiffLine {
            tag: DiffTag::Add,
            content: "new content".to_string(),
            old_line_num: None,
            new_line_num: Some(11),
        };
        let right_rendered = render_split_line(&add_line, SplitSide::Right);
        assert_eq!(right_rendered.spans[0].content, "  11");
        assert_eq!(right_rendered.spans[2].content, "+");
        assert_eq!(right_rendered.spans[3].content, "new content");

        let equal_line = DiffLine {
            tag: DiffTag::Equal,
            content: "context".to_string(),
            old_line_num: Some(5),
            new_line_num: Some(5),
        };
        let left_rendered = render_split_line(&equal_line, SplitSide::Left);
        assert_eq!(left_rendered.spans[0].content, "   5");
        assert_eq!(left_rendered.spans[2].content, " ");

        let right_rendered = render_split_line(&equal_line, SplitSide::Right);
        assert_eq!(right_rendered.spans[0].content, "   5");
        assert_eq!(right_rendered.spans[2].content, " ");
    }

    #[test]
    fn test_header_line_rendering() {
        let header = DiffLine {
            tag: DiffTag::Header,
            content: "@@ -1,3 +1,4 @@".to_string(),
            old_line_num: None,
            new_line_num: None,
        };
        let unified = render_unified_line(&header);
        assert_eq!(unified.spans.len(), 1);
        assert_eq!(unified.spans[0].content, "@@ -1,3 +1,4 @@");

        let split_left = render_split_line(&header, SplitSide::Left);
        assert_eq!(split_left.spans.len(), 1);
        assert_eq!(split_left.spans[0].content, "@@ -1,3 +1,4 @@");
    }
}
