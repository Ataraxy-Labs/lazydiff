use std::{
    env, fs,
    io::{Read as _, Write as _},
    net::{Shutdown, TcpListener, TcpStream},
};

use color_eyre::Result;
use lazydiff_v2_core::AppCore;
use lazydiff_v2_protocol::{Viewport, WorkspaceKind};

fn main() -> Result<()> {
    color_eyre::install()?;
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "--help" | "-h" | "help"))
    {
        println!("{}", help_text());
        return Ok(());
    }
    let (patch_path, port) = parse_args(&args)?;
    let patch = fs::read_to_string(patch_path)?;
    let app = AppCore::from_patch_text(&patch, WorkspaceKind::PatchFile)?;
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let address = listener.local_addr()?;
    println!("lazydiff-v2-server listening on http://{address}");

    for stream in listener.incoming() {
        respond(stream?, &app)?;
    }
    Ok(())
}

fn parse_args(args: &[String]) -> Result<(&str, u16)> {
    let mut port = 4097;
    let Some(command) = args.first().map(String::as_str) else {
        return Err(color_eyre::eyre::eyre!(
            "missing command\n\n{}",
            help_text()
        ));
    };
    if command != "patch" {
        return Err(color_eyre::eyre::eyre!(
            "unknown command `{command}`\n\n{}",
            help_text()
        ));
    }
    let Some(path) = args.get(1).map(String::as_str) else {
        return Err(color_eyre::eyre::eyre!(
            "missing patch file\n\n{}",
            help_text()
        ));
    };
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--port" => {
                port = args
                    .get(index + 1)
                    .ok_or_else(|| color_eyre::eyre::eyre!("missing --port value"))?
                    .parse()?;
                index += 1;
            }
            other => return Err(color_eyre::eyre::eyre!("unknown option `{other}`")),
        }
        index += 1;
    }
    Ok((path, port))
}

fn respond(mut stream: TcpStream, app: &AppCore) -> Result<()> {
    let mut request = [0; 1024];
    let _ = stream.read(&mut request)?;
    let body = serde_json::to_string(&app.frame(Viewport {
        first_row: 0,
        height: 200,
    }))?;
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    stream.shutdown(Shutdown::Write)?;
    Ok(())
}

fn help_text() -> &'static str {
    "Usage: lazydiff-v2-server patch <file> [--port <port>]\n\nStarts the local LazyDiff v2 runtime server."
}
