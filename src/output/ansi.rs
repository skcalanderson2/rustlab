//! Parse ANSI/SGR escape sequences (colored tracebacks, stream output) into
//! styled spans renderable with `iced::widget::rich_text`.

use anstyle_parse::{DefaultCharAccumulator, Params, Parser, Perform};
use iced::Color;

#[derive(Debug, Clone, PartialEq)]
pub struct AnsiSpan {
    pub text: String,
    pub color: Option<Color>,
    pub bold: bool,
}

/// Parse text possibly containing ANSI escapes into styled spans.
/// Non-SGR escape sequences are dropped.
pub fn parse(input: &str) -> Vec<AnsiSpan> {
    let mut performer = Collector::default();
    let mut parser = Parser::<DefaultCharAccumulator>::new();
    for byte in input.as_bytes() {
        parser.advance(&mut performer, *byte);
    }
    performer.flush();
    performer.spans
}

#[derive(Default)]
struct Collector {
    spans: Vec<AnsiSpan>,
    current: String,
    color: Option<Color>,
    bold: bool,
}

impl Collector {
    fn flush(&mut self) {
        if !self.current.is_empty() {
            self.spans.push(AnsiSpan {
                text: std::mem::take(&mut self.current),
                color: self.color,
                bold: self.bold,
            });
        }
    }

    fn apply_sgr(&mut self, params: &Params) {
        let codes: Vec<Vec<u16>> = params.iter().map(|p| p.to_vec()).collect();
        let mut i = 0;
        while i < codes.len() {
            let code = *codes[i].first().unwrap_or(&0);
            match code {
                0 => {
                    self.color = None;
                    self.bold = false;
                }
                1 => self.bold = true,
                22 => self.bold = false,
                30..=37 => self.color = Some(basic_color(code - 30, false)),
                90..=97 => self.color = Some(basic_color(code - 90, true)),
                39 => self.color = None,
                38 => {
                    // 38;5;n (indexed) or 38;2;r;g;b (truecolor), possibly as
                    // separate params or colon subparams.
                    if codes[i].len() >= 3 && codes[i][1] == 5 {
                        self.color = Some(indexed_color(codes[i][2]));
                    } else if codes[i].len() >= 5 && codes[i][1] == 2 {
                        self.color = Some(rgb(codes[i][2], codes[i][3], codes[i][4]));
                    } else if codes.len() > i + 2 && codes[i + 1] == [5] {
                        self.color = Some(indexed_color(*codes[i + 2].first().unwrap_or(&0)));
                        i += 2;
                    } else if codes.len() > i + 4 && codes[i + 1] == [2] {
                        self.color = Some(rgb(
                            *codes[i + 2].first().unwrap_or(&0),
                            *codes[i + 3].first().unwrap_or(&0),
                            *codes[i + 4].first().unwrap_or(&0),
                        ));
                        i += 4;
                    }
                }
                // Backgrounds and other attributes are ignored for now.
                _ => {}
            }
            i += 1;
        }
    }
}

impl Perform for Collector {
    fn print(&mut self, c: char) {
        self.current.push(c);
    }

    fn execute(&mut self, byte: u8) {
        if byte == b'\n' {
            self.current.push('\n');
        } else if byte == b'\t' {
            self.current.push('\t');
        }
    }

    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], ignore: bool, action: u8) {
        if !ignore && action == b'm' {
            self.flush();
            self.apply_sgr(params);
        }
    }
}

fn basic_color(index: u16, bright: bool) -> Color {
    // Palette loosely matching JupyterLab's terminal colors.
    let (r, g, b) = match (index, bright) {
        (0, false) => (0x3e, 0x42, 0x4b),
        (1, false) => (0xe7, 0x5c, 0x58),
        (2, false) => (0x00, 0xa2, 0x50),
        (3, false) => (0xdd, 0xb6, 0x2b),
        (4, false) => (0x20, 0x8f, 0xfb),
        (5, false) => (0xd1, 0x60, 0xc4),
        (6, false) => (0x60, 0xc6, 0xc8),
        (7, false) => (0xc5, 0xc1, 0xb4),
        (0, true) => (0x68, 0x6a, 0x66),
        (1, true) => (0xff, 0x6e, 0x67),
        (2, true) => (0x5a, 0xf7, 0x8e),
        (3, true) => (0xf4, 0xf9, 0x9d),
        (4, true) => (0x57, 0xc7, 0xff),
        (5, true) => (0xff, 0x6a, 0xc1),
        (6, true) => (0x9a, 0xed, 0xfe),
        (7, true) => (0xf1, 0xf1, 0xf0),
        _ => (0xc5, 0xc1, 0xb4),
    };
    Color::from_rgb8(r, g, b)
}

fn indexed_color(index: u16) -> Color {
    match index {
        0..=7 => basic_color(index, false),
        8..=15 => basic_color(index - 8, true),
        16..=231 => {
            let index = index - 16;
            let scale = |v: u16| if v == 0 { 0u8 } else { (55 + v * 40) as u8 };
            let r = scale(index / 36);
            let g = scale((index % 36) / 6);
            let b = scale(index % 6);
            Color::from_rgb8(r, g, b)
        }
        232..=255 => {
            let v = (8 + (index - 232) * 10) as u8;
            Color::from_rgb8(v, v, v)
        }
        _ => Color::WHITE,
    }
}

fn rgb(r: u16, g: u16, b: u16) -> Color {
    Color::from_rgb8(r as u8, g as u8, b as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_passthrough() {
        let spans = parse("hello world");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "hello world");
        assert_eq!(spans[0].color, None);
    }

    #[test]
    fn colored_traceback_fragment() {
        let spans = parse("\u{1b}[0;31mZeroDivisionError\u{1b}[0m: division by zero");
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].text, "ZeroDivisionError");
        assert!(spans[0].color.is_some());
        assert_eq!(spans[1].text, ": division by zero");
        assert_eq!(spans[1].color, None);
    }

    #[test]
    fn newlines_survive() {
        let spans = parse("line1\nline2");
        assert_eq!(spans[0].text, "line1\nline2");
    }

    #[test]
    fn indexed_256_color() {
        let spans = parse("\u{1b}[38;5;196mred\u{1b}[0m");
        assert_eq!(spans[0].text, "red");
        assert!(spans[0].color.is_some());
    }
}
