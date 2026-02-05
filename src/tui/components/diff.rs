//! Diff widget for displaying file changes in collapsed/expanded view.

#![allow(dead_code)]

use std::fmt;

use super::highlighting;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

/// Threshold width for switching between split and unified diff views.
/// Width >= `SPLIT_THRESHOLD` uses split view, otherwise unified view.
#[allow(dead_code)]
pub const SPLIT_THRESHOLD: u16 = 100;

/// Foreground color for added line markers (+)
pub const DIFF_ADD_FG: Color = Color::Rgb(100, 180, 100);
/// Foreground color for deleted line markers (-)
pub const DIFF_DEL_FG: Color = Color::Rgb(220, 100, 100);
/// Background color for added lines (subtle green tint)
const DIFF_ADD_BG: Color = Color::Rgb(35, 50, 35);
/// Background color for deleted lines (subtle red tint)
const DIFF_DEL_BG: Color = Color::Rgb(55, 35, 35);

/// Color for hunk headers (blue)
pub const DIFF_HUNK: Color = Color::Rgb(80, 140, 180);
/// Color for line numbers in gutter (dimmed)
const DIMMED: Color = Color::Rgb(100, 100, 110);
/// Color for file path headers
const FILE_PATH_FG: Color = Color::Rgb(200, 200, 220);

fn diff_extension(diff: &ParsedDiff) -> Option<&str> {
    diff.file_path
        .as_deref()
        .and_then(|path| path.rsplit('.').next())
}

const fn diff_background(tag: DiffTag) -> Option<Color> {
    match tag {
        DiffTag::Add => Some(DIFF_ADD_BG),
        DiffTag::Delete => Some(DIFF_DEL_BG),
        _ => None,
    }
}

fn highlight_line_spans(content: &str, language: Option<&str>) -> Vec<Span<'static>> {
    let mut spans = match language {
        Some(language) => highlighting::highlight_code(content, language),
        None => vec![Span::raw(content.to_owned())],
    };

    if !content.ends_with('\n') {
        while matches!(spans.last(), Some(span) if span.content.as_ref() == "\n") {
            spans.pop();
        }
    }

    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }

    spans
}

fn apply_background(spans: Vec<Span<'static>>, background: Option<Color>) -> Vec<Span<'static>> {
    let Some(background) = background else {
        return spans;
    };

    spans
        .into_iter()
        .map(|span| Span::styled(span.content, span.style.bg(background)))
        .collect()
}

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
pub struct DiffView {
    /// Parsed diff data
    pub diff: ParsedDiff,
    /// Whether the diff is expanded
    pub is_expanded: bool,
}

impl DiffView {
    /// Create a new diff view from a diff string
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
pub fn parse_diff(diff_str: &str) -> ParsedDiff {
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
pub fn render_unified_line(line: &DiffLine, extension: Option<&str>) -> Line<'static> {
    let old_num = line
        .old_line_num
        .map_or("    ".to_string(), |n| format!("{n:>4}"));
    let new_num = line
        .new_line_num
        .map_or("    ".to_string(), |n| format!("{n:>4}"));

    let (prefix, prefix_style) = match line.tag {
        DiffTag::Add => ("+", Style::default().fg(DIFF_ADD_FG)),
        DiffTag::Delete => ("-", Style::default().fg(DIFF_DEL_FG)),
        DiffTag::Equal => (" ", Style::default()),
        DiffTag::Header => ("", Style::default().fg(DIFF_HUNK)),
    };

    let gutter_style = Style::default().fg(DIMMED);

    if line.tag == DiffTag::Header {
        Line::from(vec![Span::styled(line.content.clone(), prefix_style)])
    } else {
        let content_spans = apply_background(
            highlight_line_spans(&line.content, extension),
            diff_background(line.tag),
        );
        let mut spans = vec![
            Span::styled(old_num, gutter_style),
            Span::raw(" "),
            Span::styled(new_num, gutter_style),
            Span::raw(" | "),
            Span::styled(prefix, prefix_style),
        ];
        spans.extend(content_spans);
        Line::from(spans)
    }
}

/// Render a diff line with line numbers in split format
#[allow(dead_code)]
pub fn render_split_line(
    line: &DiffLine,
    side: SplitSide,
    extension: Option<&str>,
) -> Line<'static> {
    let (line_num, prefix_style) = match (side, line.tag) {
        (SplitSide::Left, DiffTag::Delete) => (line.old_line_num, Style::default().fg(DIFF_DEL_FG)),
        (SplitSide::Left, DiffTag::Equal) => (line.old_line_num, Style::default()),
        (SplitSide::Right, DiffTag::Add) => (line.new_line_num, Style::default().fg(DIFF_ADD_FG)),
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
        let content_spans = apply_background(
            highlight_line_spans(&line.content, extension),
            diff_background(line.tag),
        );
        let mut spans = vec![
            Span::styled(num_str, gutter_style),
            Span::raw(" | "),
            Span::styled(prefix, prefix_style),
        ];
        spans.extend(content_spans);
        Line::from(spans)
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
pub fn render_diff(diff: &ParsedDiff, width: u16) -> Vec<Line<'static>> {
    let extension = diff_extension(diff);
    if width >= SPLIT_THRESHOLD {
        render_split_view(diff, width, extension)
    } else {
        render_unified_view(diff, extension)
    }
}

fn render_unified_view(diff: &ParsedDiff, extension: Option<&str>) -> Vec<Line<'static>> {
    let mut result = Vec::new();

    if let Some(ref path) = diff.file_path {
        result.push(Line::from(Span::styled(
            format!(" {path}"),
            Style::default().fg(FILE_PATH_FG),
        )));
    }

    let gutter_style = Style::default().fg(DIMMED);

    for line in &diff.lines {
        if line.tag == DiffTag::Header {
            continue;
        }

        let (prefix, prefix_style) = match line.tag {
            DiffTag::Add => ("+", Style::default().fg(DIFF_ADD_FG)),
            DiffTag::Delete => ("-", Style::default().fg(DIFF_DEL_FG)),
            DiffTag::Equal | DiffTag::Header => (" ", Style::default()),
        };

        let old_num = line
            .old_line_num
            .map_or("    ".to_string(), |n| format!("{n:>4}"));
        let new_num = line
            .new_line_num
            .map_or("    ".to_string(), |n| format!("{n:>4}"));

        let content_spans = apply_background(
            highlight_line_spans(&line.content, extension),
            diff_background(line.tag),
        );
        let mut spans = vec![
            Span::styled(old_num, gutter_style),
            Span::raw(" "),
            Span::styled(new_num, gutter_style),
            Span::styled(format!(" {prefix} "), prefix_style),
        ];
        spans.extend(content_spans);
        result.push(Line::from(spans));
    }

    result
}

struct SplitLine {
    content: String,
    line_num: Option<u32>,
    line_type: DiffTag,
}

fn render_split_view(diff: &ParsedDiff, width: u16, extension: Option<&str>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let gutter_style = Style::default().fg(DIMMED);

    if let Some(ref path) = diff.file_path {
        lines.push(Line::from(Span::styled(
            format!(" {path}"),
            Style::default().fg(FILE_PATH_FG),
        )));
    }

    let mut left_lines: Vec<SplitLine> = Vec::new();
    let mut right_lines: Vec<SplitLine> = Vec::new();

    let mut i = 0;
    while i < diff.lines.len() {
        let line = &diff.lines[i];

        match line.tag {
            DiffTag::Header => {
                i += 1;
            }
            DiffTag::Equal => {
                left_lines.push(SplitLine {
                    content: line.content.clone(),
                    line_num: line.old_line_num,
                    line_type: DiffTag::Equal,
                });
                right_lines.push(SplitLine {
                    content: line.content.clone(),
                    line_num: line.new_line_num,
                    line_type: DiffTag::Equal,
                });
                i += 1;
            }
            DiffTag::Delete | DiffTag::Add => {
                let mut removes: Vec<(String, Option<u32>)> = Vec::new();
                let mut adds: Vec<(String, Option<u32>)> = Vec::new();

                while i < diff.lines.len() {
                    let current = &diff.lines[i];
                    match current.tag {
                        DiffTag::Delete => {
                            removes.push((current.content.clone(), current.old_line_num));
                            i += 1;
                        }
                        DiffTag::Add => {
                            adds.push((current.content.clone(), current.new_line_num));
                            i += 1;
                        }
                        _ => break,
                    }
                }

                let max_len = removes.len().max(adds.len());
                for j in 0..max_len {
                    if j < removes.len() {
                        left_lines.push(SplitLine {
                            content: removes[j].0.clone(),
                            line_num: removes[j].1,
                            line_type: DiffTag::Delete,
                        });
                    } else {
                        left_lines.push(SplitLine {
                            content: String::new(),
                            line_num: None,
                            line_type: DiffTag::Header,
                        });
                    }

                    if j < adds.len() {
                        right_lines.push(SplitLine {
                            content: adds[j].0.clone(),
                            line_num: adds[j].1,
                            line_type: DiffTag::Add,
                        });
                    } else {
                        right_lines.push(SplitLine {
                            content: String::new(),
                            line_num: None,
                            line_type: DiffTag::Header,
                        });
                    }
                }
            }
        }
    }

    let gutter_width: u16 = 7;
    let separator_width: u16 = 3;
    let total_gutter = gutter_width * 2 + separator_width;
    let content_width = width.saturating_sub(total_gutter);
    let col_width = (content_width / 2) as usize;

    for (left, right) in left_lines.iter().zip(right_lines.iter()) {
        let left_num = left
            .line_num
            .map_or("    ".to_string(), |n| format!("{n:>4}"));
        let right_num = right
            .line_num
            .map_or("    ".to_string(), |n| format!("{n:>4}"));

        let (left_marker, left_marker_style) = match left.line_type {
            DiffTag::Delete => ("-", Style::default().fg(DIFF_DEL_FG)),
            _ => (" ", Style::default()),
        };
        let (right_marker, right_marker_style) = match right.line_type {
            DiffTag::Add => ("+", Style::default().fg(DIFF_ADD_FG)),
            _ => (" ", Style::default()),
        };

        let left_bg = match left.line_type {
            DiffTag::Delete => Some(DIFF_DEL_BG),
            _ => None,
        };
        let right_bg = match right.line_type {
            DiffTag::Add => Some(DIFF_ADD_BG),
            _ => None,
        };

        let left_content = truncate_to_width(&left.content, col_width);
        let right_content = truncate_to_width(&right.content, col_width);

        let left_spans = apply_background(highlight_line_spans(&left_content, extension), left_bg);
        let right_spans =
            apply_background(highlight_line_spans(&right_content, extension), right_bg);

        let mut spans = Vec::new();
        spans.push(Span::styled(left_num, gutter_style));
        spans.push(Span::styled(format!("{left_marker} "), left_marker_style));
        spans.extend(left_spans);

        let left_rendered_len: usize = left_content.chars().count();
        if left_rendered_len < col_width {
            let padding = " ".repeat(col_width - left_rendered_len);
            if let Some(bg) = left_bg {
                spans.push(Span::styled(padding, Style::default().bg(bg)));
            } else {
                spans.push(Span::raw(padding));
            }
        }

        spans.push(Span::styled(" | ", gutter_style));
        spans.push(Span::styled(right_num, gutter_style));
        spans.push(Span::styled(format!("{right_marker} "), right_marker_style));
        spans.extend(right_spans);

        let right_rendered_len: usize = right_content.chars().count();
        if right_rendered_len < col_width {
            let padding = " ".repeat(col_width - right_rendered_len);
            if let Some(bg) = right_bg {
                spans.push(Span::styled(padding, Style::default().bg(bg)));
            } else {
                spans.push(Span::raw(padding));
            }
        }

        lines.push(Line::from(spans));
    }

    lines
}

fn truncate_to_width(s: &str, width: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= width {
        s.to_string()
    } else if width > 1 {
        chars[..width - 1].iter().collect::<String>() + "…"
    } else {
        "…".to_string()
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
        let rendered = render_unified_line(&equal_line, None);
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
        let rendered = render_unified_line(&delete_line, None);
        assert_eq!(rendered.spans[0].content, "   2");
        assert_eq!(rendered.spans[2].content, "    ");
        assert_eq!(rendered.spans[4].content, "-");

        let add_line = DiffLine {
            tag: DiffTag::Add,
            content: "    new line".to_string(),
            old_line_num: None,
            new_line_num: Some(2),
        };
        let rendered = render_unified_line(&add_line, None);
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
        let left_rendered = render_split_line(&delete_line, SplitSide::Left, None);
        assert_eq!(left_rendered.spans[0].content, "  10");
        assert_eq!(left_rendered.spans[2].content, "-");
        assert_eq!(left_rendered.spans[3].content, "old content");

        let add_line = DiffLine {
            tag: DiffTag::Add,
            content: "new content".to_string(),
            old_line_num: None,
            new_line_num: Some(11),
        };
        let right_rendered = render_split_line(&add_line, SplitSide::Right, None);
        assert_eq!(right_rendered.spans[0].content, "  11");
        assert_eq!(right_rendered.spans[2].content, "+");
        assert_eq!(right_rendered.spans[3].content, "new content");

        let equal_line = DiffLine {
            tag: DiffTag::Equal,
            content: "context".to_string(),
            old_line_num: Some(5),
            new_line_num: Some(5),
        };
        let left_rendered = render_split_line(&equal_line, SplitSide::Left, None);
        assert_eq!(left_rendered.spans[0].content, "   5");
        assert_eq!(left_rendered.spans[2].content, " ");

        let right_rendered = render_split_line(&equal_line, SplitSide::Right, None);
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
        let unified = render_unified_line(&header, None);
        assert_eq!(unified.spans.len(), 1);
        assert_eq!(unified.spans[0].content, "@@ -1,3 +1,4 @@");

        let split_left = render_split_line(&header, SplitSide::Left, None);
        assert_eq!(split_left.spans.len(), 1);
        assert_eq!(split_left.spans[0].content, "@@ -1,3 +1,4 @@");
    }

    #[test]
    fn test_render_diff_unified_at_narrow_width() {
        let diff = "--- test.txt\n+++ test.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let parsed = parse_diff(diff);

        let lines = render_diff(&parsed, 80);
        assert_eq!(lines.len(), 3);

        assert!(lines[0].spans[0].content.contains("test.txt"));
    }

    #[test]
    fn test_render_diff_split_at_wide_width() {
        let diff = "--- test.txt\n+++ test.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let parsed = parse_diff(diff);

        let lines = render_diff(&parsed, 120);
        assert_eq!(lines.len(), 2);

        assert!(lines[0].spans[0].content.contains("test.txt"));
        assert!(lines[1]
            .spans
            .iter()
            .any(|span| span.content.contains(" | ")));
    }

    #[test]
    fn test_render_diff_threshold_boundary() {
        let diff = "--- test.txt\n+++ test.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let parsed = parse_diff(diff);

        let unified = render_diff(&parsed, 99);
        let split = render_diff(&parsed, 100);

        assert_eq!(unified.len(), 3);
        assert_eq!(split.len(), 2);
    }

    #[test]
    fn test_render_diff_split_delete_only() {
        let diff = "--- test.txt\n+++ test.txt\n@@ -1,2 +1 @@\n-deleted\n context\n";
        let parsed = parse_diff(diff);

        let lines = render_diff(&parsed, 120);
        assert_eq!(lines.len(), 3);

        let delete_line = &lines[1];
        let delete_text: String = delete_line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(delete_text.contains("deleted"));
    }

    #[test]
    fn test_render_diff_split_add_only() {
        let diff = "--- test.txt\n+++ test.txt\n@@ -1 +1,2 @@\n context\n+added\n";
        let parsed = parse_diff(diff);

        let lines = render_diff(&parsed, 120);
        assert_eq!(lines.len(), 3);

        let add_line = &lines[2];
        let add_text: String = add_line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(add_text.contains("added"));
    }
}
