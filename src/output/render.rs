//! Render cell outputs (mime bundles, streams, errors) as iced elements.

use base64::Engine;
use iced::widget::{column, image, markdown, rich_text, span, svg, text};
use iced::{Element, Font};
use jupyter_protocol::{Media, MediaType};

use crate::notebook::model::CellOutput;
use crate::output::ansi::{self, AnsiSpan};

const OUTPUT_TEXT_SIZE: f32 = 13.0;

/// A mime bundle converted once, at insert/load time, into something the view
/// can render every frame without re-decoding or re-parsing.
#[derive(Debug)]
pub enum Rendered {
    Image(image::Handle),
    Svg(svg::Handle),
    Markdown(markdown::Content),
    Ansi(Vec<AnsiSpan>),
    Unsupported(&'static str),
}

/// Pick the richest natively renderable representation and prepare it.
/// HTML has no native renderer; the bundle's `text/plain` is used instead.
pub fn prepare(media: &Media) -> Rendered {
    let richest = media.richest(|mime| match mime {
        MediaType::Png(_) => 7,
        MediaType::Jpeg(_) => 6,
        MediaType::Gif(_) => 5,
        MediaType::Svg(_) => 4,
        MediaType::Markdown(_) => 3,
        MediaType::Latex(_) => 2,
        MediaType::Plain(_) => 1,
        _ => 0,
    });
    match richest {
        Some(MediaType::Png(b64)) | Some(MediaType::Jpeg(b64)) | Some(MediaType::Gif(b64)) => {
            match decode_base64(b64) {
                Some(bytes) => Rendered::Image(image::Handle::from_bytes(bytes)),
                None => Rendered::Unsupported("image (invalid base64)"),
            }
        }
        Some(MediaType::Svg(markup)) => {
            Rendered::Svg(svg::Handle::from_memory(markup.clone().into_bytes()))
        }
        Some(MediaType::Markdown(md)) => Rendered::Markdown(markdown::Content::parse(md)),
        Some(MediaType::Latex(tex)) => Rendered::Ansi(ansi::parse(tex)),
        Some(MediaType::Plain(s)) => Rendered::Ansi(ansi::parse(s)),
        Some(_) => Rendered::Unsupported("output"),
        None => Rendered::Unsupported("output"),
    }
}

fn decode_base64(data: &str) -> Option<Vec<u8>> {
    let cleaned: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD.decode(cleaned).ok()
}

pub fn view_output<'a>(output: &'a CellOutput, dark: bool) -> Element<'a, markdown::Uri> {
    match output {
        CellOutput::Stream { spans, .. } => ansi_text(spans),
        CellOutput::Data { rendered, .. } => view_rendered(rendered, dark),
        CellOutput::Error { spans, .. } => {
            column(spans.iter().map(|line| ansi_text(line))).into()
        }
    }
}

fn view_rendered<'a>(rendered: &'a Rendered, dark: bool) -> Element<'a, markdown::Uri> {
    match rendered {
        Rendered::Image(handle) => image(handle.clone()).into(),
        Rendered::Svg(handle) => svg(handle.clone())
            .width(iced::Shrink)
            .height(iced::Shrink)
            .into(),
        Rendered::Markdown(content) => markdown::view(
            content.items(),
            markdown::Settings::with_text_size(
                14,
                if dark { iced::Theme::Dark } else { iced::Theme::Light },
            ),
        ),
        Rendered::Ansi(spans) => ansi_text(spans),
        Rendered::Unsupported(kind) => text(format!("<unsupported {kind}>"))
            .font(Font::MONOSPACE)
            .size(OUTPUT_TEXT_SIZE)
            .into(),
    }
}

/// Turn pre-parsed ANSI spans into a rich_text element. Borrows the span
/// text — no per-frame parsing or string cloning.
fn ansi_text<'a, Message: 'a + Clone>(spans: &'a [AnsiSpan]) -> Element<'a, Message> {
    let spans: Vec<iced::widget::text::Span<'a, ()>> = spans
        .iter()
        .map(|s| {
            let mut sp = span(s.text.as_str())
                .font(Font::MONOSPACE)
                .size(OUTPUT_TEXT_SIZE);
            if let Some(color) = s.color {
                sp = sp.color(color);
            }
            sp
        })
        .collect();
    rich_text(spans).into()
}
