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

/// JupyterLab-style tile: white card, hairline outline, gentle hover.
fn tile_style(theme: &iced::Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(if hovered {
            palette.background.weak.color.into()
        } else {
            palette.background.base.color.into()
        }),
        text_color: palette.background.base.text,
        border: iced::Border {
            color: if hovered {
                palette.primary.weak.color
            } else {
                palette.background.strong.color.scale_alpha(0.5)
            },
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: if hovered {
            iced::Shadow {
                color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.12),
                offset: iced::Vector::new(0.0, 1.0),
                blur_radius: 4.0,
            }
        } else {
            iced::Shadow::default()
        },
        ..button::Style::default()
    }
}

fn tile<'a>(glyph: &'a str, label: String, event: Event) -> Element<'a, Event> {
    button(
        column![
            container(text(glyph).size(30)).height(48).center_y(Fill),
            text(label)
                .size(11)
                .center()
                .wrapping(text::Wrapping::WordOrGlyph),
        ]
        .spacing(6)
        .align_x(iced::Center)
        .width(Fill),
    )
    .style(tile_style)
    .width(140)
    .height(120)
    .padding([10, 6])
    .clip(true)
    .on_press(event)
    .into()
}

pub fn view(specs: &[KernelspecDir]) -> Element<'_, Event> {
    let section = |title: &'static str, make: fn(String) -> Event| {
        let tiles = row(specs.iter().map(|spec| {
            let display = if spec.kernelspec.display_name.is_empty() {
                spec.kernel_name.clone()
            } else {
                spec.kernelspec.display_name.clone()
            };
            tile(
                kernel_glyph(&spec.kernelspec.language),
                display,
                make(spec.kernel_name.clone()),
            )
        }))
        .spacing(12)
        .wrap();

        column![text(title).size(17), tiles].spacing(12)
    };

    let content = column![
        text("Launcher").size(24),
        section("Notebook", Event::NewNotebook),
        section("Console", Event::NewConsole),
        column![
            text("Other").size(17),
            tile("$_", "Terminal".to_string(), Event::NewTerminal)
        ]
        .spacing(12),
    ]
    .spacing(28)
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
