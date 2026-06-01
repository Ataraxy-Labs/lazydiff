use std::{
    io::{ErrorKind, Read as _, Write as _},
    net::{Shutdown, TcpStream},
};

use color_eyre::Result;
use lazydiff_v2_protocol::AppFrame;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientWorkspace {
    frame: AppFrame,
}

impl ClientWorkspace {
    pub fn from_frame(frame: AppFrame) -> Self {
        Self { frame }
    }

    pub fn frame(&self) -> &AppFrame {
        &self.frame
    }

    pub fn terminal_text(&self) -> String {
        render_text("LazyDiff v2 terminal", &self.frame)
    }

    pub fn gpui_text(&self) -> String {
        render_text("LazyDiff v2 GPUI", &self.frame)
    }

    pub fn debug_snapshot(&self, renderer: &'static str) -> ClientDebugSnapshot {
        ClientDebugSnapshot {
            renderer,
            title: if renderer == "gpui" {
                "LazyDiff v2 GPUI"
            } else {
                "LazyDiff v2 terminal"
            },
            total_rows: self.frame.diff.total_rows,
            visible_rows: self
                .frame
                .diff
                .rows
                .iter()
                .map(|row| row.text.clone())
                .collect(),
            commands: self
                .frame
                .commands
                .iter()
                .map(|command| command.id.clone())
                .collect(),
            keymaps: self
                .frame
                .keymaps
                .iter()
                .map(|keymap| format!("{} -> {}", keymap.key, keymap.command))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClientDebugSnapshot {
    pub renderer: &'static str,
    pub title: &'static str,
    pub total_rows: usize,
    pub visible_rows: Vec<String>,
    pub commands: Vec<String>,
    pub keymaps: Vec<String>,
}

pub fn fetch_frame(server: &str) -> Result<AppFrame> {
    let server = server.trim_start_matches("http://");
    let mut stream = TcpStream::connect(server)?;
    write!(
        stream,
        "GET /frame HTTP/1.1\r\nhost: {server}\r\nconnection: close\r\n\r\n"
    )?;
    stream.shutdown(Shutdown::Write)?;
    let response = read_response(&mut stream)?;
    let response = String::from_utf8(response)?;
    let (_, body) = response
        .split_once("\r\n\r\n")
        .or_else(|| response.split_once("\n\n"))
        .ok_or_else(|| color_eyre::eyre::eyre!("invalid server response: {:?}", response))?;
    Ok(serde_json::from_str(body)?)
}

fn read_response(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut response = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(bytes_read) => response.extend_from_slice(&buffer[..bytes_read]),
            Err(error) if error.kind() == ErrorKind::ConnectionReset && !response.is_empty() => {
                break;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(response)
}

fn render_text(title: &str, frame: &AppFrame) -> String {
    let mut output = String::new();
    output.push_str(title);
    output.push('\n');
    output.push_str(&format!("rows: {}\n", frame.diff.total_rows));
    for row in &frame.diff.rows {
        output.push_str(&row.text);
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use lazydiff_v2_protocol::{DiffFrame, DiffRow, DiffRowKind, SurfaceId, WorkspaceKind};

    use super::*;

    #[test]
    fn shared_client_core_renders_text_for_both_hosts() {
        let workspace = ClientWorkspace::from_frame(AppFrame {
            active_surface: SurfaceId::Diff,
            workspace_kind: WorkspaceKind::PatchFile,
            diff: DiffFrame {
                total_rows: 1,
                rows: vec![DiffRow {
                    visual_index: 0,
                    kind: DiffRowKind::FileHeader,
                    text: "src/main.rs".to_string(),
                }],
            },
            commands: vec![],
            keymaps: vec![],
        });

        assert!(workspace.terminal_text().contains("LazyDiff v2 terminal"));
        assert!(workspace.gpui_text().contains("LazyDiff v2 GPUI"));
        assert_eq!(
            workspace.debug_snapshot("gpui").visible_rows,
            vec!["src/main.rs"]
        );
    }
}
