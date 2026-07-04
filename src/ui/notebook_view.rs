//! Notebook document view: toolbar + cell list.

use iced::widget::{button, column, container, markdown, mouse_area, row, scrollable, text, text_editor};
use iced::{Element, Fill, Font, Theme};

use crate::notebook::model::{CellKind, CellState, NotebookDoc};
use crate::output::render;

#[derive(Debug, Clone)]
pub enum Event {
    CellAction(usize, text_editor::Action),
    RunCell(usize),
    EditMarkdown(usize),
    LinkClicked(markdown::Uri),
    Save,
    Interrupt,
    Restart,
}

pub struct KernelIndicator<'a> {
    pub label: &'a str,
    pub busy: Option<bool>,
}

pub fn view<'a>(
    doc: &'a NotebookDoc,
    language: &'a str,
    kernel: KernelIndicator<'a>,
) -> Element<'a, Event> {
    let status = match kernel.busy {
        Some(true) => format!("{} ●", kernel.label),
        Some(false) => format!("{} ○", kernel.label),
        None => kernel.label.to_string(),
    };

    let toolbar = row![
        button(text("💾").size(13)).padding(6).on_press(Event::Save),
        button(text("⏹").size(13)).padding(6).on_press(Event::Interrupt),
        button(text("⟳").size(13)).padding(6).on_press(Event::Restart),
        container(text(status).size(13)).padding(6),
    ]
    .spacing(6)
    .padding(6);

    let cells = column(
        doc.cells
            .iter()
            .enumerate()
            .map(|(i, cell)| view_cell(i, cell, language)),
    )
    .spacing(12)
    .padding(16);

    column![toolbar, scrollable(cells).width(Fill).height(Fill)].into()
}

fn view_cell<'a>(index: usize, cell: &'a CellState, language: &'a str) -> Element<'a, Event> {
    match &cell.kind {
        CellKind::Code => {
            let gutter_label = if cell.running {
                "In [*]:".to_string()
            } else {
                match cell.execution_count {
                    Some(n) => format!("In [{n}]:"),
                    None => "In [ ]:".to_string(),
                }
            };
            let gutter = column![
                text(gutter_label).font(Font::MONOSPACE).size(12),
                button(text("▶").size(12)).on_press(Event::RunCell(index)),
            ]
            .spacing(4)
            .width(70);

            let editor = code_editor(index, cell, language);

            let mut body = column![row![gutter, editor].spacing(8)].spacing(8);
            if !cell.outputs.is_empty() {
                let outputs = column(
                    cell.outputs
                        .iter()
                        .map(|o| render::view_output(o).map(Event::LinkClicked)),
                )
                .spacing(4)
                .padding([0, 78]);
                body = body.push(outputs);
            }
            body.into()
        }
        CellKind::Markdown { rendered, editing } => {
            if *editing {
                container(code_editor(index, cell, "markdown"))
                    .padding([0, 78])
                    .width(Fill)
                    .into()
            } else {
                mouse_area(
                    container(
                        markdown::view(
                            rendered.items(),
                            markdown::Settings::with_text_size(14, Theme::Light),
                        )
                        .map(Event::LinkClicked),
                    )
                    .padding([0, 78])
                    .width(Fill),
                )
                .on_double_click(Event::EditMarkdown(index))
                .into()
            }
        }
        CellKind::Raw => container(text(cell.source_text()).font(Font::MONOSPACE).size(13))
            .padding([0, 78])
            .into(),
    }
}

fn code_editor<'a>(index: usize, cell: &'a CellState, language: &'a str) -> Element<'a, Event> {
    text_editor(&cell.source)
        .placeholder("...")
        .font(Font::MONOSPACE)
        .size(14)
        .highlight(language, iced::highlighter::Theme::InspiredGitHub)
        .on_action(move |action| Event::CellAction(index, action))
        .key_binding(move |key_press| {
            use iced::keyboard::key::{Key, Named};
            if matches!(key_press.key, Key::Named(Named::Enter)) && key_press.modifiers.shift() {
                return Some(text_editor::Binding::Custom(Event::RunCell(index)));
            }
            text_editor::Binding::from_key_press(key_press)
        })
        .into()
}
