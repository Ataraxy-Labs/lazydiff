use std::{env, fs, path::PathBuf};

use color_eyre::Result;
use gpui::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _, Window,
    WindowOptions, div,
};
use gpui_component::{ActiveTheme as _, Root, StyledExt as _};
use lazydiff_v2_client::{ClientWorkspace, fetch_frame};

struct LazyDiffGui {
    text: String,
}

impl Render for LazyDiffGui {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = div()
            .v_flex()
            .gap_1()
            .size_full()
            .p_4()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground);

        for line in self.text.lines() {
            body = body.child(line.to_string());
        }

        body
    }
}

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
    let options = parse_args(&args)?;
    let workspace = ClientWorkspace::from_frame(fetch_frame(options.server)?);
    if let Some(path) = options.debug_dump {
        let snapshot = workspace.debug_snapshot("gpui");
        fs::write(path, serde_json::to_string_pretty(&snapshot)?)?;
    }
    let text = workspace.gpui_text();
    gpui_platform::application().run(move |cx: &mut App| {
        gpui_component::init(cx);
        let text = text.clone();
        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|_| LazyDiffGui { text: text.clone() });
                cx.new(|cx| Root::new(view, window, cx).bg(cx.theme().background))
            })
            .expect("failed to open LazyDiff v2 GPUI window");
        })
        .detach();
    });
    Ok(())
}

struct Options<'a> {
    server: &'a str,
    debug_dump: Option<PathBuf>,
}

fn parse_args(args: &[String]) -> Result<Options<'_>> {
    let mut server = "127.0.0.1:4097";
    let mut debug_dump = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--server" => {
                server = args
                    .get(index + 1)
                    .map(String::as_str)
                    .ok_or_else(|| color_eyre::eyre::eyre!("missing --server value"))?;
                index += 1;
            }
            "--debug-dump" => {
                debug_dump =
                    Some(PathBuf::from(args.get(index + 1).ok_or_else(|| {
                        color_eyre::eyre::eyre!("missing --debug-dump value")
                    })?));
                index += 1;
            }
            _ => {
                return Err(color_eyre::eyre::eyre!(
                    "unknown lazydiff-gui-v2 arguments\n\n{}",
                    help_text()
                ));
            }
        }
        index += 1;
    }
    Ok(Options { server, debug_dump })
}

fn help_text() -> &'static str {
    "Usage: lazydiff-gui-v2 [--server <host:port>] [--debug-dump <path>]\n\nConnects to the local LazyDiff v2 server and renders its semantic frame in GPUI."
}
