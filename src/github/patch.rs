use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct RawPullRequestFile {
    filename: String,
    previous_filename: Option<String>,
    status: Option<String>,
    patch: Option<String>,
}

pub(super) fn parse_pull_request_files_value(
    value: serde_json::Value,
) -> std::result::Result<Vec<RawPullRequestFile>, String> {
    match value {
        serde_json::Value::Array(values) => {
            if values.first().is_some_and(|value| value.is_array()) {
                values
                    .into_iter()
                    .flat_map(|value| match value {
                        serde_json::Value::Array(page) => page,
                        other => vec![other],
                    })
                    .map(serde_json::from_value)
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())
            } else {
                values
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())
            }
        }
        other => serde_json::from_value(other)
            .map_err(|error| format!("unexpected PR files JSON: {error}")),
    }
}

pub(super) fn pull_request_files_to_patch(files: &[RawPullRequestFile]) -> String {
    files
        .iter()
        .map(file_header_patch)
        .collect::<Vec<_>>()
        .join("\n")
}

fn file_header_patch(file: &RawPullRequestFile) -> String {
    let old_path = file.previous_filename.as_deref().unwrap_or(&file.filename);
    let new_path = file.filename.as_str();
    let status = file.status.as_deref().unwrap_or_default();
    let old_ref = if status == "added" {
        "/dev/null".to_string()
    } else {
        prefixed_diff_path("a", old_path)
    };
    let new_ref = if status == "removed" {
        "/dev/null".to_string()
    } else {
        prefixed_diff_path("b", new_path)
    };
    let mut lines = vec![
        format!(
            "diff --git {} {}",
            prefixed_diff_path("a", old_path),
            prefixed_diff_path("b", new_path)
        ),
        format!("--- {old_ref}"),
        format!("+++ {new_ref}"),
    ];
    if status == "renamed" && file.previous_filename.is_some() {
        lines.insert(1, format!("rename from {old_path}"));
        lines.insert(2, format!("rename to {new_path}"));
    }
    if let Some(patch) = file
        .patch
        .as_deref()
        .filter(|patch| !patch.trim().is_empty())
    {
        lines.push(patch.trim_end().to_string());
    }
    lines.join("\n")
}

fn prefixed_diff_path(prefix: &str, path: &str) -> String {
    diff_path(&format!("{prefix}/{path}"))
}

fn diff_path(path: &str) -> String {
    if path.chars().any(|ch| ch.is_whitespace() || ch == '"') {
        serde_json::to_string(path).unwrap_or_else(|_| path.to_string())
    } else {
        path.to_string()
    }
}
