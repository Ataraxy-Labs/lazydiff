use crate::{DiffLine, FileDiff};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDiffKind {
    Change,
    New,
    Deleted,
    RenamePure,
    RenameChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiffMetadata {
    pub name: String,
    pub prev_name: Option<String>,
    pub kind: FileDiffKind,
    pub hunks: Vec<HunkMetadata>,
    pub addition_lines: usize,
    pub deletion_lines: usize,
    pub split_line_count: usize,
    pub unified_line_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunkMetadata {
    pub collapsed_before: u32,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub deletion_line_index: usize,
    pub addition_line_index: usize,
    pub split_line_start: usize,
    pub split_line_count: usize,
    pub unified_line_start: usize,
    pub unified_line_count: usize,
    pub hunk_specs: String,
    pub content: Vec<HunkContent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkContent {
    Context {
        lines: u32,
        deletion_line_index: usize,
        addition_line_index: usize,
    },
    Change {
        deletions: u32,
        additions: u32,
        deletion_line_index: usize,
        addition_line_index: usize,
    },
}

impl HunkContent {
    pub fn split_line_count(&self) -> usize {
        match self {
            Self::Context { lines, .. } => *lines as usize,
            Self::Change {
                deletions,
                additions,
                ..
            } => (*deletions).max(*additions) as usize,
        }
    }

    pub fn unified_line_count(&self) -> usize {
        match self {
            Self::Context { lines, .. } => *lines as usize,
            Self::Change {
                deletions,
                additions,
                ..
            } => (*deletions + *additions) as usize,
        }
    }
}

pub fn build_file_metadata(file: &FileDiff) -> FileDiffMetadata {
    let mut hunks = Vec::with_capacity(file.hunks.len());
    let mut previous_old_end = 0;
    let mut split_line_count = 0;
    let mut unified_line_count = 0;
    let mut deletion_line_index = 0;
    let mut addition_line_index = 0;
    let mut addition_lines = 0;
    let mut deletion_lines = 0;

    for hunk in &file.hunks {
        let collapsed_before = hunk.old_start.saturating_sub(previous_old_end + 1);
        let content = build_hunk_content(
            hunk.lines.as_slice(),
            deletion_line_index,
            addition_line_index,
        );
        let hunk_split_lines = content.iter().map(HunkContent::split_line_count).sum();
        let hunk_unified_lines = content.iter().map(HunkContent::unified_line_count).sum();
        let old_count = hunk
            .lines
            .iter()
            .filter(|line| matches!(line, DiffLine::Context { .. } | DiffLine::Delete { .. }))
            .count() as u32;
        let new_count = hunk
            .lines
            .iter()
            .filter(|line| matches!(line, DiffLine::Context { .. } | DiffLine::Add { .. }))
            .count() as u32;

        hunks.push(HunkMetadata {
            collapsed_before,
            old_start: hunk.old_start,
            old_count,
            new_start: hunk.new_start,
            new_count,
            deletion_line_index,
            addition_line_index,
            split_line_start: split_line_count + collapsed_before as usize,
            split_line_count: hunk_split_lines,
            unified_line_start: unified_line_count + collapsed_before as usize,
            unified_line_count: hunk_unified_lines,
            hunk_specs: hunk.header.clone(),
            content,
        });

        split_line_count += collapsed_before as usize + hunk_split_lines;
        unified_line_count += collapsed_before as usize + hunk_unified_lines;
        deletion_line_index += old_count as usize;
        addition_line_index += new_count as usize;
        deletion_lines += hunk
            .lines
            .iter()
            .filter(|line| matches!(line, DiffLine::Delete { .. }))
            .count();
        addition_lines += hunk
            .lines
            .iter()
            .filter(|line| matches!(line, DiffLine::Add { .. }))
            .count();
        previous_old_end = hunk.old_start + old_count.saturating_sub(1);
    }

    FileDiffMetadata {
        name: file.new_path.clone(),
        prev_name: file.old_path.clone().filter(|prev| prev != &file.new_path),
        kind: file_kind(file),
        hunks,
        addition_lines,
        deletion_lines,
        split_line_count,
        unified_line_count,
    }
}

fn build_hunk_content(
    lines: &[DiffLine],
    mut deletion_line_index: usize,
    mut addition_line_index: usize,
) -> Vec<HunkContent> {
    let mut content = Vec::new();
    let mut line_index = 0;

    while line_index < lines.len() {
        match lines[line_index] {
            DiffLine::Context { .. } => {
                let start_deletion = deletion_line_index;
                let start_addition = addition_line_index;
                let mut count = 0;
                while line_index < lines.len()
                    && matches!(lines[line_index], DiffLine::Context { .. })
                {
                    count += 1;
                    deletion_line_index += 1;
                    addition_line_index += 1;
                    line_index += 1;
                }
                content.push(HunkContent::Context {
                    lines: count,
                    deletion_line_index: start_deletion,
                    addition_line_index: start_addition,
                });
            }
            DiffLine::Delete { .. } | DiffLine::Add { .. } => {
                let start_deletion = deletion_line_index;
                let start_addition = addition_line_index;
                let mut deletions = 0;
                let mut additions = 0;
                while line_index < lines.len()
                    && matches!(
                        lines[line_index],
                        DiffLine::Delete { .. } | DiffLine::Add { .. }
                    )
                {
                    match lines[line_index] {
                        DiffLine::Delete { .. } => {
                            deletions += 1;
                            deletion_line_index += 1;
                        }
                        DiffLine::Add { .. } => {
                            additions += 1;
                            addition_line_index += 1;
                        }
                        DiffLine::Context { .. } => unreachable!(),
                    }
                    line_index += 1;
                }
                content.push(HunkContent::Change {
                    deletions,
                    additions,
                    deletion_line_index: start_deletion,
                    addition_line_index: start_addition,
                });
            }
        }
    }

    content
}

fn file_kind(file: &FileDiff) -> FileDiffKind {
    let has_additions = file
        .hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .any(|line| matches!(line, DiffLine::Add { .. }));
    let has_deletions = file
        .hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .any(|line| matches!(line, DiffLine::Delete { .. }));
    let renamed = file
        .old_path
        .as_ref()
        .is_some_and(|old_path| old_path != &file.new_path);

    match (renamed, has_additions, has_deletions) {
        (true, false, false) => FileDiffKind::RenamePure,
        (true, _, _) => FileDiffKind::RenameChanged,
        (false, true, false) => FileDiffKind::New,
        (false, false, true) => FileDiffKind::Deleted,
        _ => FileDiffKind::Change,
    }
}
