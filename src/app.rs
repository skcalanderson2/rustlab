//! The iced application: state, messages, update loop, and (for now) a
//! minimal single-notebook view. Grows into the JupyterLab shell in M4.

use std::collections::HashMap;
use std::path::PathBuf;

use iced::futures::stream;
use iced::widget::{
    button, column, container, markdown, row, scrollable, text, text_editor,
};
use iced::{Element, Fill, Font, Subscription, Task, Theme};
use jupyter_protocol::{ExecuteRequest, JupyterMessage, JupyterMessageContent, ExecutionState};
use tokio::sync::mpsc;

use crate::kernel::discovery::{self, KernelspecDir};
use crate::kernel::worker::{self, KernelCommand, KernelEvent, KernelHandle};
use crate::notebook::model::{self, CellKind, CellOutput, NotebookDoc};
use crate::output::render;

pub fn run(path: Option<PathBuf>) -> iced::Result {
    iced::application(move || App::boot(path.clone()), App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .run()
}

pub struct App {
    doc: Option<NotebookDoc>,
    kernel: KernelState,
    /// execute_request msg_id → cell index, for routing iopub messages.
    pending: HashMap<String, usize>,
    status_line: String,
}

enum KernelState {
    Idle,
    Launching { spec_name: String },
    Ready { handle: KernelHandle, busy: bool },
    Dead { reason: String },
}

#[derive(Debug, Clone)]
pub enum Message {
    CellAction(usize, text_editor::Action),
    RunCell(usize),
    EditMarkdown(usize),
    Kernel(KernelMsg),
    Save,
    Saved(Result<(), String>),
    LinkClicked(markdown::Uri),
}

#[derive(Debug, Clone)]
pub enum KernelMsg {
    Ready(KernelHandle),
    Failed(String),
    Event(KernelEvent),
}

impl std::fmt::Debug for KernelHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KernelHandle")
            .field("session_id", &self.session_id)
            .finish()
    }
}

impl App {
    fn boot(path: Option<PathBuf>) -> (Self, Task<Message>) {
        let mut status_line = String::new();
        let doc = match &path {
            Some(p) => match model::load(p) {
                Ok(doc) => Some(doc),
                Err(e) => {
                    status_line = format!("failed to open {}: {e:#}", p.display());
                    None
                }
            },
            None => None,
        };

        let kernel_task = match &doc {
            Some(doc) => {
                let preferred = doc
                    .metadata
                    .kernelspec
                    .as_ref()
                    .map(|k| k.name.clone());
                Task::stream(kernel_events(preferred)).map(Message::Kernel)
            }
            None => Task::none(),
        };

        let app = App {
            doc,
            kernel: KernelState::Launching {
                spec_name: String::from("..."),
            },
            pending: HashMap::new(),
            status_line,
        };
        (app, kernel_task)
    }

    fn theme(&self) -> Theme {
        Theme::Light
    }

    fn title(&self) -> String {
        match &self.doc {
            Some(doc) => {
                let name = doc
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Untitled.ipynb".to_string());
                let dirty = if doc.dirty { "● " } else { "" };
                format!("{dirty}{name} — RustLab")
            }
            None => "RustLab".to_string(),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::CellAction(index, action) => {
                if let Some(doc) = &mut self.doc {
                    if let Some(cell) = doc.cells.get_mut(index) {
                        let is_edit = action.is_edit();
                        cell.source.perform(action);
                        if is_edit {
                            doc.dirty = true;
                        }
                    }
                }
                Task::none()
            }
            Message::RunCell(index) => self.run_cell(index),
            Message::EditMarkdown(index) => {
                if let Some(cell) = self.doc.as_mut().and_then(|d| d.cells.get_mut(index)) {
                    if let CellKind::Markdown { editing, .. } = &mut cell.kind {
                        *editing = true;
                    }
                }
                Task::none()
            }
            Message::Kernel(msg) => self.on_kernel_msg(msg),
            Message::Save => self.save(),
            Message::Saved(Ok(())) => {
                self.status_line = "saved".to_string();
                Task::none()
            }
            Message::Saved(Err(e)) => {
                self.status_line = format!("save failed: {e}");
                Task::none()
            }
            Message::LinkClicked(uri) => {
                let _ = open::that(uri.to_string());
                Task::none()
            }
        }
    }

    fn run_cell(&mut self, index: usize) -> Task<Message> {
        // "Running" a markdown cell renders it, like in JupyterLab.
        if let Some(cell) = self.doc.as_mut().and_then(|d| d.cells.get_mut(index)) {
            if let CellKind::Markdown { rendered, editing } = &mut cell.kind {
                *rendered = markdown::Content::parse(&cell.source.text());
                *editing = false;
                return Task::none();
            }
        }

        let KernelState::Ready { handle, .. } = &self.kernel else {
            self.status_line = "kernel not ready".to_string();
            return Task::none();
        };
        let Some(doc) = &mut self.doc else {
            return Task::none();
        };
        let Some(cell) = doc.cells.get_mut(index) else {
            return Task::none();
        };
        if !cell.is_code() {
            return Task::none();
        }

        let code = cell.source_text();
        let exec: JupyterMessage = ExecuteRequest::new(code).into();
        self.pending.insert(exec.header.msg_id.clone(), index);
        cell.outputs.clear();
        cell.running = true;
        doc.dirty = true;

        let commands = handle.commands.clone();
        Task::future(async move {
            let _ = commands.send(KernelCommand::Shell(exec)).await;
        })
        .discard()
    }

    fn save(&mut self) -> Task<Message> {
        let Some(doc) = &mut self.doc else {
            return Task::none();
        };
        let Some(path) = doc.path.clone() else {
            return Task::none();
        };
        match model::save(doc) {
            Ok(json) => {
                doc.dirty = false;
                Task::perform(
                    async move {
                        tokio::fs::write(&path, json)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::Saved,
                )
            }
            Err(e) => {
                self.status_line = format!("serialize failed: {e:#}");
                Task::none()
            }
        }
    }

    fn on_kernel_msg(&mut self, msg: KernelMsg) -> Task<Message> {
        match msg {
            KernelMsg::Ready(handle) => {
                self.status_line = format!("kernel ready ({})", handle.connection_info.kernel_name.as_deref().unwrap_or("?"));
                self.kernel = KernelState::Ready {
                    handle,
                    busy: false,
                };
            }
            KernelMsg::Failed(e) => {
                self.status_line = format!("kernel failed: {e}");
                self.kernel = KernelState::Dead { reason: e };
            }
            KernelMsg::Event(KernelEvent::Exited(code)) => {
                self.kernel = KernelState::Dead {
                    reason: format!("exited (code {code:?})"),
                };
            }
            KernelMsg::Event(KernelEvent::ShellReply(reply)) => {
                if let JupyterMessageContent::ExecuteReply(r) = &reply.content {
                    let parent = reply.parent_header.as_ref().map(|h| h.msg_id.clone());
                    if let Some(index) = parent.and_then(|id| self.pending.get(&id).copied()) {
                        if let Some(cell) = self.doc.as_mut().and_then(|d| d.cells.get_mut(index)) {
                            cell.execution_count = Some(r.execution_count.value() as i32);
                        }
                    }
                }
            }
            KernelMsg::Event(KernelEvent::IoPub(msg)) => self.on_iopub(msg),
        }
        Task::none()
    }

    fn on_iopub(&mut self, msg: JupyterMessage) {
        let parent_id = msg.parent_header.as_ref().map(|h| h.msg_id.clone());
        let cell_index = parent_id.as_ref().and_then(|id| self.pending.get(id)).copied();

        if let JupyterMessageContent::Status(s) = &msg.content {
            if let KernelState::Ready { busy, .. } = &mut self.kernel {
                *busy = s.execution_state == ExecutionState::Busy;
            }
            if s.execution_state == ExecutionState::Idle {
                if let (Some(index), Some(id)) = (cell_index, parent_id.as_ref()) {
                    self.pending.remove(id);
                    if let Some(cell) = self.doc.as_mut().and_then(|d| d.cells.get_mut(index)) {
                        cell.running = false;
                    }
                }
            }
            return;
        }

        let Some(index) = cell_index else { return };
        let Some(cell) = self.doc.as_mut().and_then(|d| d.cells.get_mut(index)) else {
            return;
        };

        match msg.content {
            JupyterMessageContent::StreamContent(s) => {
                let name = match s.name {
                    jupyter_protocol::Stdio::Stdout => "stdout",
                    jupyter_protocol::Stdio::Stderr => "stderr",
                };
                cell.push_stream(name, &s.text);
            }
            JupyterMessageContent::ExecuteInput(input) => {
                cell.execution_count = Some(input.execution_count.value() as i32);
            }
            JupyterMessageContent::ExecuteResult(r) => {
                cell.outputs.push(CellOutput::Data {
                    rendered: render::prepare(&r.data),
                    media: r.data,
                    execution_count: Some(r.execution_count),
                });
            }
            JupyterMessageContent::DisplayData(d) => {
                cell.outputs.push(CellOutput::Data {
                    rendered: render::prepare(&d.data),
                    media: d.data,
                    execution_count: None,
                });
            }
            JupyterMessageContent::ErrorOutput(e) => {
                cell.outputs.push(CellOutput::Error {
                    ename: e.ename,
                    evalue: e.evalue,
                    traceback: e.traceback,
                });
            }
            _ => {}
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        iced::event::listen_with(|event, _status, _window| {
            if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Character(c),
                modifiers,
                ..
            }) = &event
            {
                if modifiers.command() && c.as_str() == "s" {
                    return Some(Message::Save);
                }
            }
            None
        })
    }

    fn view(&self) -> Element<'_, Message> {
        let Some(doc) = &self.doc else {
            return container(text(if self.status_line.is_empty() {
                "No notebook. Run: rustlab <path.ipynb>"
            } else {
                &self.status_line
            }))
            .center(Fill)
            .into();
        };

        let kernel_status = match &self.kernel {
            KernelState::Idle => text("no kernel"),
            KernelState::Launching { .. } => text("kernel: starting..."),
            KernelState::Ready { busy: false, .. } => text("kernel: idle ○"),
            KernelState::Ready { busy: true, .. } => text("kernel: busy ●"),
            KernelState::Dead { reason } => text(format!("kernel dead: {reason}")),
        };

        let toolbar = row![
            button("Save").on_press(Message::Save),
            kernel_status,
            text(&self.status_line).size(13),
        ]
        .spacing(16)
        .padding(8);

        let cells = column(
            doc.cells
                .iter()
                .enumerate()
                .map(|(i, cell)| self.view_cell(i, cell)),
        )
        .spacing(12)
        .padding(16);

        column![toolbar, scrollable(cells).width(Fill).height(Fill)].into()
    }

    fn view_cell<'a>(
        &'a self,
        index: usize,
        cell: &'a crate::notebook::model::CellState,
    ) -> Element<'a, Message> {
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
                    button(text("▶").size(12)).on_press(Message::RunCell(index)),
                ]
                .spacing(4)
                .width(70);

                let editor = text_editor(&cell.source)
                    .placeholder("...")
                    .font(Font::MONOSPACE)
                    .size(14)
                    .highlight("python", iced::highlighter::Theme::InspiredGitHub)
                    .on_action(move |action| Message::CellAction(index, action))
                    .key_binding(move |key_press| {
                        use iced::keyboard::key::{Key, Named};
                        if matches!(key_press.key, Key::Named(Named::Enter))
                            && key_press.modifiers.shift()
                        {
                            return Some(text_editor::Binding::Custom(Message::RunCell(index)));
                        }
                        text_editor::Binding::from_key_press(key_press)
                    });

                let mut body = column![row![gutter, editor].spacing(8)].spacing(8);
                if !cell.outputs.is_empty() {
                    let outputs = column(
                        cell.outputs
                            .iter()
                            .map(|o| render::view_output(o).map(Message::LinkClicked)),
                    )
                    .spacing(4)
                    .padding([0, 78]);
                    body = body.push(outputs);
                }
                body.into()
            }
            CellKind::Markdown { rendered, editing } => {
                if *editing {
                    let editor = text_editor(&cell.source)
                        .placeholder("Type markdown...")
                        .font(Font::MONOSPACE)
                        .size(14)
                        .highlight("markdown", iced::highlighter::Theme::InspiredGitHub)
                        .on_action(move |action| Message::CellAction(index, action))
                        .key_binding(move |key_press| {
                            use iced::keyboard::key::{Key, Named};
                            if matches!(key_press.key, Key::Named(Named::Enter))
                                && key_press.modifiers.shift()
                            {
                                return Some(text_editor::Binding::Custom(Message::RunCell(
                                    index,
                                )));
                            }
                            text_editor::Binding::from_key_press(key_press)
                        });
                    container(editor).padding([0, 78]).width(Fill).into()
                } else {
                    iced::widget::mouse_area(
                        container(
                            markdown::view(
                                rendered.items(),
                                markdown::Settings::with_text_size(14, Theme::Light),
                            )
                            .map(Message::LinkClicked),
                        )
                        .padding([0, 78])
                        .width(Fill),
                    )
                    .on_double_click(Message::EditMarkdown(index))
                    .into()
                }
            }
            CellKind::Raw => container(
                text(cell.source_text()).font(Font::MONOSPACE).size(13),
            )
            .padding([0, 78])
            .into(),
        }
    }
}

/// A stream that launches a kernel, then yields everything it emits.
fn kernel_events(preferred_spec: Option<String>) -> impl stream::Stream<Item = KernelMsg> {
    enum St {
        Launch(Option<String>),
        Run(mpsc::Receiver<KernelEvent>),
        Done,
    }

    stream::unfold(St::Launch(preferred_spec), |st| async move {
        match st {
            St::Launch(preferred) => {
                let spec = match find_spec(preferred).await {
                    Ok(spec) => spec,
                    Err(e) => return Some((KernelMsg::Failed(format!("{e:#}")), St::Done)),
                };
                match worker::launch(spec).await {
                    Ok((handle, rx)) => Some((KernelMsg::Ready(handle), St::Run(rx))),
                    Err(e) => Some((KernelMsg::Failed(format!("{e:#}")), St::Done)),
                }
            }
            St::Run(mut rx) => rx
                .recv()
                .await
                .map(|event| (KernelMsg::Event(event), St::Run(rx))),
            St::Done => None,
        }
    })
}

async fn find_spec(preferred: Option<String>) -> anyhow::Result<KernelspecDir> {
    let specs = discovery::list_kernelspecs().await;
    anyhow::ensure!(!specs.is_empty(), "no jupyter kernels installed");
    let spec = preferred
        .and_then(|name| specs.iter().find(|s| s.kernel_name == name).cloned())
        .or_else(|| specs.iter().find(|s| s.kernel_name == "python3").cloned())
        .or_else(|| {
            specs
                .iter()
                .find(|s| s.kernelspec.language == "python")
                .cloned()
        })
        .unwrap_or_else(|| specs[0].clone());
    Ok(spec)
}
