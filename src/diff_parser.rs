use crate::models::{DiffLine, DiffLineKind, ParsedDiff};

pub fn parse_git_diff(raw_diff: &str) -> ParsedDiff {
    let mut lines = Vec::new();
    let mut additions = 0;
    let mut deletions = 0;
    let mut max_line_len = 0;

    let mut cur_old_line: Option<usize> = None;
    let mut cur_new_line: Option<usize> = None;

    const MAX_DIFF_LINES: usize = 25_000;

    for line in raw_diff.lines() {
        if lines.len() >= MAX_DIFF_LINES {
            lines.push(DiffLine {
                kind: DiffLineKind::Meta,
                old_line: None,
                new_line: None,
                content: "... [Diff truncated for performance. Remaining lines omitted] ...".to_string(),
            });
            break;
        }

        max_line_len = max_line_len.max(line.len());

        if line.starts_with("diff --git")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("new file mode")
            || line.starts_with("deleted file mode")
            || line.starts_with("similarity index")
            || line.starts_with("rename from")
            || line.starts_with("rename to")
            || line.starts_with("Binary files ")
        {
            lines.push(DiffLine {
                kind: DiffLineKind::Meta,
                old_line: None,
                new_line: None,
                content: line.to_string(),
            });
        } else if line.starts_with("@@") {
            // Hunk header: e.g. @@ -1,5 +1,6 @@ optional title
            let (old_start, new_start) = parse_hunk_header(line);
            cur_old_line = Some(old_start);
            cur_new_line = Some(new_start);

            lines.push(DiffLine {
                kind: DiffLineKind::HunkHeader,
                old_line: None,
                new_line: None,
                content: line.to_string(),
            });
        } else if let Some(content) = line.strip_prefix('+') {
            additions += 1;
            let n_line = cur_new_line;
            if let Some(n) = cur_new_line {
                cur_new_line = Some(n + 1);
            }
            lines.push(DiffLine {
                kind: DiffLineKind::Addition,
                old_line: None,
                new_line: n_line,
                content: content.to_string(),
            });
        } else if let Some(content) = line.strip_prefix('-') {
            deletions += 1;
            let o_line = cur_old_line;
            if let Some(o) = cur_old_line {
                cur_old_line = Some(o + 1);
            }
            lines.push(DiffLine {
                kind: DiffLineKind::Deletion,
                old_line: o_line,
                new_line: None,
                content: content.to_string(),
            });
        } else if line.starts_with('\\') {
            lines.push(DiffLine {
                kind: DiffLineKind::Meta,
                old_line: None,
                new_line: None,
                content: line.to_string(),
            });
        } else {
            // Context line
            let content = line.strip_prefix(' ').unwrap_or(line);
            let o_line = cur_old_line;
            let n_line = cur_new_line;
            if let Some(o) = cur_old_line {
                cur_old_line = Some(o + 1);
            }
            if let Some(n) = cur_new_line {
                cur_new_line = Some(n + 1);
            }
            lines.push(DiffLine {
                kind: DiffLineKind::Context,
                old_line: o_line,
                new_line: n_line,
                content: content.to_string(),
            });
        }
    }

    ParsedDiff {
        lines,
        additions,
        deletions,
        max_line_len,
    }
}

fn parse_hunk_header(header: &str) -> (usize, usize) {
    // @@ -1,5 +1,6 @@
    let mut old_start = 1;
    let mut new_start = 1;

    let parts: Vec<&str> = header.split("@@").collect();
    if parts.len() >= 2 {
        let hunk_spec = parts[1].trim();
        for token in hunk_spec.split_whitespace() {
            if let Some(rest) = token.strip_prefix('-') {
                let num_str = rest.split(',').next().unwrap_or(rest);
                if let Ok(val) = num_str.parse::<usize>() {
                    old_start = val;
                }
            } else if let Some(rest) = token.strip_prefix('+') {
                let num_str = rest.split(',').next().unwrap_or(rest);
                if let Ok(val) = num_str.parse::<usize>() {
                    new_start = val;
                }
            }
        }
    }

    (old_start, new_start)
}
