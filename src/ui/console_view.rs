//! Console (REPL) tab: scrollback of executed entries + input editor.

use iced::widget::{column, container, markdown, row, scrollable, text, text_editor};
use iced::{Element, Fill, Font};

use crate::notebook::model::CellOutput;
use crate::output::render;

#[derive(Debug, Clone)]
pub enum Event {
    InputAction(text_editor::Action),
    Submit,
    LinkClicked(markdown::Uri),
}

pub struct ConsoleEntry {
    pub execution_count: Option<i32>,
    pub source: String,
    pub outputs: Vec<CellOutput>,
    pub running: bool,
}

pub fn view<'a>(
    entries: &'a [ConsoleEntry],
    input: &'a text_editor::Content,
    language: &'a str,
    kernel_label: &'a str,
    busy: Option<bool>,
) -> Element<'a, Event> {
    let status = match busy {
        Some(true) => format!("{kernel_label} ●"),
        Some(false) => format!("{kernel_label} ○"),
        None => kernel_label.to_string(),
    };

    let scrollback = column(entries.iter().map(view_entry))
        .spacing(12)
        .padding(16)
        .width(Fill);

    let prompt = text(">>>").font(Font::MONOSPACE).size(14);
    let editor = text_editor(input)
        .placeholder("Shift+Enter to run")
        .font(Font::MONOSPACE)
        .size(14)
        .highlight(language, iced::highlighter::Theme::InspiredGitHub)
        .on_action(Event::InputAction)
        .key_binding(|key_press| {
            use iced::keyboard::key::{Key, Named};
            if matches!(key_press.key, Key::Named(Named::Enter)) && key_press.modifiers.shift() {
                return Some(text_editor::Binding::Custom(Event::Submit));
            }
            text_editor::Binding::from_key_press(key_press)
        });

    column![
        container(text(status).size(13)).padding(6),
        scrollable(scrollback).height(Fill).anchor_bottom(),
        container(row![prompt, editor].spacing(8)).padding(12),
    ]
    .into()
}

fn view_entry(entry: &ConsoleEntry) -> Element<'_, Event> {
    let label = if entry.running {
        "In [*]:".to_string()
    } else {
        match entry.execution_count {
            Some(n) => format!("In [{n}]:"),
            None => "In [ ]:".to_string(),
        }
    };

    let mut body = column![
        row![
            text(label).font(Font::MONOSPACE).size(12).width(70),
            text(&entry.source).font(Font::MONOSPACE).size(14),
        ]
        .spacing(8)
    ]
    .spacing(6);

    if !entry.outputs.is_empty() {
        body = body.push(
            column(
                entry
                    .outputs
                    .iter()
                    .map(|o| render::view_output(o).map(Event::LinkClicked)),
            )
            .spacing(4)
            .padding([0, 78]),
        );
    }
    body.into()
}
