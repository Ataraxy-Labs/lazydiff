use std::{
    env,
    io::{ErrorKind, Read as _, Write as _},
    net::{Shutdown, TcpStream},
};

use color_eyre::Result;
use lazydiff_v2_protocol::AppFrame;

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
    let server = parse_server(&args)?;
    let frame = fetch_frame(server)?;
    print!("{}", render_terminal(&frame));
    Ok(())
}

fn parse_server(args: &[String]) -> Result<&str> {
    match args {
        [flag, server] if flag == "--server" => Ok(server),
        [] => Ok("127.0.0.1:4097"),
        _ => Err(color_eyre::eyre::eyre!(
            "unknown lazydiff-v2 arguments\n\n{}",
            help_text()
        )),
    }
}

fn fetch_frame(server: &str) -> Result<AppFrame> {
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
            Err(error) => {
                return Err(error.into());
            }
        }
    }
    Ok(response)
}

fn render_terminal(frame: &AppFrame) -> String {
    let mut output = String::new();
    output.push_str("LazyDiff v2 terminal\n");
    output.push_str(&format!("rows: {}\n", frame.diff.total_rows));
    for row in &frame.diff.rows {
        output.push_str(&row.text);
        output.push('\n');
    }
    output
}

fn help_text() -> &'static str {
    "Usage: lazydiff-v2 [--server <host:port>]\n\nConnects to the local LazyDiff v2 server and renders its semantic frame."
}
