use std::env;

use color_eyre::Result;
use lazydiff_v2_client::{ClientWorkspace, fetch_frame};

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
    let workspace = ClientWorkspace::from_frame(fetch_frame(server)?);
    print!("{}", workspace.terminal_text());
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

fn help_text() -> &'static str {
    "Usage: lazydiff-v2 [--server <host:port>]\n\nConnects to the local LazyDiff v2 server and renders its semantic frame."
}
