//! Render cell outputs (mime bundles, streams, errors) as iced elements.

use base64::Engine;
use iced::widget::{column, image, markdown, rich_text, span, svg, text};
use iced::{Color, Element, Font};
use jupyter_protocol::{Media, MediaType};

use crate::notebook::model::CellOutput;
use crate::output::ansi;

const ERROR_RED: Color = Color::from_rgb(0.86, 0.36, 0.34);
const OUTPUT_TEXT_SIZE: f32 = 13.0;

/// A mime bundle converted once, at insert/load time, into something the view
/// can render every frame without re-decoding.
#[derive(Debug)]
pub enum Rendered {
    Image(image::Handle),
    Svg(svg::Handle),
    Markdown(markdown::Content),
    Text(String),
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
        Some(MediaType::Latex(tex)) => Rendered::Text(tex.clone()),
        Some(MediaType::Plain(s)) => Rendered::Text(s.clone()),
        Some(_) => Rendered::Unsupported("output"),
        None => Rendered::Unsupported("output"),
    }
}

fn decode_base64(data: &str) -> Option<Vec<u8>> {
    let cleaned: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD.decode(cleaned).ok()
}

pub fn view_output<'a>(output: &'a CellOutput) -> Element<'a, markdown::Uri> {
    match output {
        CellOutput::Stream { text, .. } => ansi_text(text),
        CellOutput::Data { rendered, .. } => view_rendered(rendered),
        CellOutput::Error {
            traceback,
            ename,
            evalue,
        } => {
            if traceback.is_empty() {
                ansi_text_colored(&format!("{ename}: {evalue}"), Some(ERROR_RED))
            } else {
                column(traceback.iter().map(|line| ansi_text(line))).into()
            }
        }
    }
}

fn view_rendered<'a>(rendered: &'a Rendered) -> Element<'a, markdown::Uri> {
    match rendered {
        Rendered::Image(handle) => image(handle.clone()).into(),
        Rendered::Svg(handle) => svg(handle.clone())
            .width(iced::Shrink)
            .height(iced::Shrink)
            .into(),
        Rendered::Markdown(content) => markdown::view(
            content.items(),
            markdown::Settings::with_text_size(14, iced::Theme::Light),
        ),
        Rendered::Text(s) => ansi_text(s),
        Rendered::Unsupported(kind) => text(format!("<unsupported {kind}>"))
            .font(Font::MONOSPACE)
            .size(OUTPUT_TEXT_SIZE)
            .into(),
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
            let mut sp = span(s.text).font(Font::MONOSPACE).size(OUTPUT_TEXT_SIZE);
            if let Some(color) = s.color.or(default_color) {
                sp = sp.color(color);
            }
            sp
        })
        .collect();
    rich_text(spans).into()
}
