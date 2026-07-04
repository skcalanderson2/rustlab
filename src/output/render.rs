//! Render cell outputs (mime bundles, streams, errors) as iced elements.

use iced::widget::{column, rich_text, span, text};
use iced::{Color, Element, Font};
use jupyter_protocol::{Media, MediaType};

use crate::notebook::model::CellOutput;
use crate::output::ansi;

const ERROR_RED: Color = Color::from_rgb(0.86, 0.36, 0.34);

pub fn view_output<'a, Message: 'a + Clone>(output: &'a CellOutput) -> Element<'a, Message> {
    match output {
        CellOutput::Stream { text, .. } => ansi_text(text),
        CellOutput::Data { media, .. } => view_media(media),
        CellOutput::Error { traceback, ename, evalue } => {
            if traceback.is_empty() {
                ansi_text_colored(&format!("{ename}: {evalue}"), Some(ERROR_RED))
            } else {
                column(traceback.iter().map(|line| ansi_text(line))).into()
            }
        }
    }
}

/// Pick the richest media type we can render natively. HTML intentionally
/// ranks below plain text for now (no native HTML renderer).
fn view_media<'a, Message: 'a + Clone>(media: &'a Media) -> Element<'a, Message> {
    let richest = media.richest(|mime| match mime {
        MediaType::Plain(_) => 1,
        _ => 0,
    });
    match richest {
        Some(MediaType::Plain(s)) => ansi_text(s),
        _ => text("<unsupported output>").font(Font::MONOSPACE).size(13).into(),
    }
}

fn ansi_text<'a, Message: 'a + Clone>(content: &str) -> Element<'a, Message> {
    ansi_text_colored(content, None)
}

fn ansi_text_colored<'a, Message: 'a + Clone>(
    content: &str,
    default_color: Option<Color>,
) -> Element<'a, Message> {
    let spans: Vec<iced::widget::text::Span<'static, ()>> = ansi::parse(content)
        .into_iter()
        .map(|s| {
            let mut sp = span(s.text).font(Font::MONOSPACE).size(13.0);
            if let Some(color) = s.color.or(default_color) {
                sp = sp.color(color);
            }
            sp
        })
        .collect();
    rich_text(spans).into()
}
