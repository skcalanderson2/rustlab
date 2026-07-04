//! Launcher tab: tiles for creating notebooks/consoles per installed kernel.

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Fill};
use jupyter_zmq_client::kernelspec::KernelspecDir;

#[derive(Debug, Clone)]
pub enum Event {
    NewNotebook(String),
    NewConsole(String),
    NewTerminal,
}

pub fn view(specs: &[KernelspecDir]) -> Element<'_, Event> {
    let section = |title: &'static str, make: fn(String) -> Event| {
        let tiles = row(specs.iter().map(|spec| {
            let display = if spec.kernelspec.display_name.is_empty() {
                spec.kernel_name.clone()
            } else {
                spec.kernelspec.display_name.clone()
            };
            button(
                column![
                    text(kernel_glyph(&spec.kernelspec.language)).size(28),
                    text(display).size(12).center(),
                ]
                .spacing(8)
                .align_x(iced::Center)
                .width(Fill),
            )
            .style(button::secondary)
            .width(130)
            .height(110)
            .padding(12)
            .on_press(make(spec.kernel_name.clone()))
            .into()
        }))
        .spacing(12)
        .wrap();

        column![text(title).size(18), tiles].spacing(12)
    };

    let terminal_tile = button(
        column![text("$_").size(28), text("Terminal").size(12).center()]
            .spacing(8)
            .align_x(iced::Center)
            .width(Fill),
    )
    .style(button::secondary)
    .width(130)
    .height(110)
    .padding(12)
    .on_press(Event::NewTerminal);

    let content = column![
        text("Launcher").size(24),
        section("Notebook", Event::NewNotebook),
        section("Console", Event::NewConsole),
        column![text("Other").size(18), terminal_tile].spacing(12),
    ]
    .spacing(24)
    .padding(32)
    .width(Fill);

    container(scrollable(content))
        .width(Fill)
        .height(Fill)
        .into()
}

fn kernel_glyph(language: &str) -> &'static str {
    match language.to_lowercase().as_str() {
        "python" => "🐍",
        "julia" => "🟣",
        "mojo" => "🔥",
        "rust" => "🦀",
        "r" => "📊",
        _ => "⚙️",
    }
}
