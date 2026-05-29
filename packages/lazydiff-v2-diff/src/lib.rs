use lazydiff_v2_protocol::{DiffFrame, DiffRow, DiffRowKind, Viewport};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffDocument {
    pub files: Vec<DiffFile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffFile {
    pub old_path: Option<String>,
    pub new_path: String,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffParseError {
    message: String,
}

impl DiffParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DiffParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DiffParseError {}

impl DiffDocument {
    pub fn parse(input: &str) -> Result<Self, DiffParseError> {
        let mut files = Vec::new();
        let mut current_file: Option<DiffFile> = None;
        let mut current_hunk: Option<DiffHunk> = None;

        for line in input.lines() {
            if let Some(rest) = line.strip_prefix("diff --git ") {
                push_hunk(&mut current_file, &mut current_hunk);
                if let Some(file) = current_file.take() {
                    files.push(file);
                }
                current_file = Some(parse_diff_git_file(rest));
            } else if let Some(path) = line.strip_prefix("--- ") {
                if let Some(file) = current_file.as_mut() {
                    file.old_path = normalize_patch_path(path);
                }
            } else if let Some(path) = line.strip_prefix("+++ ") {
                if let Some(file) = current_file.as_mut() {
                    if let Some(path) = normalize_patch_path(path) {
                        file.new_path = path;
                    }
                }
            } else if line.starts_with("@@") {
                push_hunk(&mut current_file, &mut current_hunk);
                ensure_file(&mut current_file);
                current_hunk = Some(DiffHunk {
                    header: line.to_string(),
                    lines: Vec::new(),
                });
            } else if let Some(hunk) = current_hunk.as_mut() {
                if line.starts_with('+') && !line.starts_with("+++") {
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Added,
                        text: line.to_string(),
                    });
                } else if line.starts_with('-') && !line.starts_with("---") {
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Removed,
                        text: line.to_string(),
                    });
                } else if line.starts_with(' ') || line.is_empty() {
                    hunk.lines.push(DiffLine {
                        kind: DiffLineKind::Context,
                        text: line.to_string(),
                    });
                }
            }
        }

        push_hunk(&mut current_file, &mut current_hunk);
        if let Some(file) = current_file.take() {
            files.push(file);
        }

        if files.is_empty() {
            return Err(DiffParseError::new("patch did not contain a unified diff"));
        }

        Ok(Self { files })
    }

    pub fn frame(&self, viewport: Viewport) -> DiffFrame {
        let rows = self.visual_rows();
        let total_rows = rows.len();
        let visible = rows
            .into_iter()
            .skip(viewport.first_row)
            .take(viewport.height)
            .collect();
        DiffFrame {
            total_rows,
            rows: visible,
        }
    }

    fn visual_rows(&self) -> Vec<DiffRow> {
        let mut rows = Vec::new();
        for file in &self.files {
            push_frame_row(&mut rows, DiffRowKind::FileHeader, file.new_path.clone());
            for hunk in &file.hunks {
                push_frame_row(&mut rows, DiffRowKind::HunkHeader, hunk.header.clone());
                for line in &hunk.lines {
                    let kind = match line.kind {
                        DiffLineKind::Context => DiffRowKind::Context,
                        DiffLineKind::Added => DiffRowKind::Added,
                        DiffLineKind::Removed => DiffRowKind::Removed,
                    };
                    push_frame_row(&mut rows, kind, line.text.clone());
                }
            }
        }
        rows
    }
}

fn push_hunk(file: &mut Option<DiffFile>, hunk: &mut Option<DiffHunk>) {
    if let (Some(file), Some(hunk)) = (file.as_mut(), hunk.take()) {
        file.hunks.push(hunk);
    }
}

fn ensure_file(file: &mut Option<DiffFile>) {
    if file.is_none() {
        *file = Some(DiffFile {
            old_path: None,
            new_path: "unknown".to_string(),
            hunks: Vec::new(),
        });
    }
}

fn parse_diff_git_file(rest: &str) -> DiffFile {
    let mut parts = rest.split_whitespace();
    let old_path = parts.next().and_then(normalize_git_path);
    let new_path = parts
        .next()
        .and_then(normalize_git_path)
        .or_else(|| old_path.clone())
        .unwrap_or_else(|| "unknown".to_string());
    DiffFile {
        old_path,
        new_path,
        hunks: Vec::new(),
    }
}

fn normalize_git_path(path: &str) -> Option<String> {
    if path == "/dev/null" {
        None
    } else {
        Some(
            path.strip_prefix("a/")
                .or_else(|| path.strip_prefix("b/"))
                .unwrap_or(path)
                .to_string(),
        )
    }
}

fn normalize_patch_path(path: &str) -> Option<String> {
    normalize_git_path(path.split_whitespace().next().unwrap_or(path))
}

fn push_frame_row(rows: &mut Vec<DiffRow>, kind: DiffRowKind, text: String) {
    rows.push(DiffRow {
        visual_index: rows.len(),
        kind,
        text,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unified_diff_files_hunks_and_lines() {
        let document = DiffDocument::parse("diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,2 +1,2 @@\n fn main() {\n-old();\n+new();\n }\n").unwrap();

        assert_eq!(document.files[0].new_path, "src/main.rs");
        assert_eq!(document.files[0].hunks[0].header, "@@ -1,2 +1,2 @@");
        assert_eq!(
            document.files[0].hunks[0].lines[1].kind,
            DiffLineKind::Removed
        );
        assert_eq!(
            document.files[0].hunks[0].lines[2].kind,
            DiffLineKind::Added
        );
    }

    #[test]
    fn slices_frame_by_viewport() {
        let document = DiffDocument::parse(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n",
        )
        .unwrap();
        let frame = document.frame(Viewport {
            first_row: 1,
            height: 2,
        });

        assert_eq!(frame.total_rows, 4);
        assert_eq!(frame.rows[0].kind, DiffRowKind::HunkHeader);
        assert_eq!(frame.rows[1].text, "-old");
    }
}
