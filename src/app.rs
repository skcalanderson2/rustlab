//! The iced application: JupyterLab-style shell — menu bar, file-browser
//! sidebar, and a pane grid of tabbed documents (Launcher, notebooks).

use std::collections::HashMap;
use std::path::PathBuf;

use iced::futures::stream;
use iced::widget::{
    button, column, container, pane_grid, row, text, text_editor,
};
use iced::{Element, Fill, Subscription, Task, Theme};
use jupyter_protocol::{
    ExecuteRequest, ExecutionState, InterruptRequest, JupyterMessage, JupyterMessageContent,
};
use tokio::sync::mpsc;

use crate::kernel::discovery::{self, KernelspecDir};
use crate::kernel::worker::{self, KernelCommand, KernelEvent, KernelHandle};
use crate::notebook::model::{self, CellKind, CellOutput, NotebookDoc};
use crate::output::render;
use crate::ui::{console_view, launcher, notebook_view, sidebar};
use std::collections::VecDeque;

pub fn run(path: Option<PathBuf>) -> iced::Result {
    iced::application(move || App::boot(path.clone()), App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .window(iced::window::Settings {
            size: iced::Size::new(1280.0, 850.0),
            ..Default::default()
        })
        .run()
}

pub type TabId = u64;

pub struct App {
    specs: Vec<KernelspecDir>,
    browser: sidebar::FileBrowser,
    panes: pane_grid::State<PaneTabs>,
    focused_pane: pane_grid::Pane,
    tabs: HashMap<TabId, Tab>,
    next_tab_id: TabId,
    status_line: String,
    /// True after a lone `d` in command mode; the next `d` deletes the cell.
    pending_delete: bool,
}

struct PaneTabs {
    tabs: Vec<TabId>,
    active: usize,
}

enum Tab {
    Launcher,
    Notebook(NotebookTab),
    Console(ConsoleTab),
}

struct NotebookTab {
    doc: NotebookDoc,
    kernel: KernelState,
    /// execute_request msg_id → cell index, for routing iopub messages.
    pending: HashMap<String, usize>,
    /// syntect token for highlighting cell sources.
    language: String,
    kernel_label: String,
    selected: usize,
    /// Cells queued by Run All, executed one at a time.
    queue: VecDeque<usize>,
}

struct ConsoleTab {
    entries: Vec<console_view::ConsoleEntry>,
    input: text_editor::Content,
    kernel: KernelState,
    pending: HashMap<String, usize>,
    language: String,
    kernel_label: String,
}

impl NotebookTab {
    fn new(doc: NotebookDoc, language: String) -> Self {
        NotebookTab {
            doc,
            kernel: KernelState::Launching,
            pending: HashMap::new(),
            language,
            kernel_label: "starting...".to_string(),
            selected: 0,
            queue: VecDeque::new(),
        }
    }
}

enum KernelState {
    Launching,
    Ready { handle: KernelHandle, busy: bool },
    Dead { reason: String },
}

#[derive(Debug, Clone)]
pub enum Message {
    SpecsLoaded(Vec<KernelspecDir>),
    Sidebar(sidebar::Event),
    Launcher(launcher::Event),
    Notebook(TabId, notebook_view::Event),
    Console(TabId, console_view::Event),
    Kernel(TabId, KernelMsg),
    CommandKey(String),
    SelectTab(pane_grid::Pane, usize),
    CloseTab(pane_grid::Pane, usize),
    NewLauncher(pane_grid::Pane),
    SplitPane(pane_grid::Pane),
    PaneResized(pane_grid::ResizeEvent),
    PaneDragged(pane_grid::DragEvent),
    PaneClicked(pane_grid::Pane),
    SaveActive,
    SavePathChosen(TabId, Option<PathBuf>),
    Saved(TabId, Result<(), String>),
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
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));

        let mut tabs = HashMap::new();
        tabs.insert(0, Tab::Launcher);
        let (panes, pane) = pane_grid::State::new(PaneTabs {
            tabs: vec![0],
            active: 0,
        });

        let mut app = App {
            specs: Vec::new(),
            browser: sidebar::FileBrowser::new(cwd.clone()),
            panes,
            focused_pane: pane,
            tabs,
            next_tab_id: 1,
            status_line: String::new(),
            pending_delete: false,
        };

        let mut tasks = vec![
            Task::perform(async { discovery::list_kernelspecs().await }, Message::SpecsLoaded),
            Task::perform(sidebar::read_dir(cwd), |r| {
                Message::Sidebar(sidebar::Event::Loaded(r))
            }),
        ];
        if let Some(path) = path {
            tasks.push(app.open_notebook(path));
        }
        (app, Task::batch(tasks))
    }

    fn theme(&self) -> Theme {
        Theme::Light
    }

    fn title(&self) -> String {
        "RustLab".to_string()
    }

    // ------------------------------------------------------------------
    // Update
    // ------------------------------------------------------------------

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SpecsLoaded(specs) => {
                self.specs = specs;
                Task::none()
            }
            Message::Sidebar(event) => self.on_sidebar(event),
            Message::Launcher(event) => self.on_launcher(event),
            Message::Notebook(tab_id, event) => self.on_notebook(tab_id, event),
            Message::Console(tab_id, event) => self.on_console(tab_id, event),
            Message::Kernel(tab_id, msg) => self.on_kernel_msg(tab_id, msg),
            Message::CommandKey(key) => self.on_command_key(&key),
            Message::SelectTab(pane, index) => {
                if let Some(state) = self.panes.get_mut(pane) {
                    state.active = index.min(state.tabs.len().saturating_sub(1));
                }
                self.focused_pane = pane;
                Task::none()
            }
            Message::CloseTab(pane, index) => self.close_tab(pane, index),
            Message::NewLauncher(pane) => {
                let id = self.alloc_tab(Tab::Launcher);
                if let Some(state) = self.panes.get_mut(pane) {
                    state.tabs.push(id);
                    state.active = state.tabs.len() - 1;
                }
                Task::none()
            }
            Message::SplitPane(pane) => {
                let id = self.alloc_tab(Tab::Launcher);
                if let Some((new_pane, _)) = self.panes.split(
                    pane_grid::Axis::Vertical,
                    pane,
                    PaneTabs {
                        tabs: vec![id],
                        active: 0,
                    },
                ) {
                    self.focused_pane = new_pane;
                }
                Task::none()
            }
            Message::PaneResized(pane_grid::ResizeEvent { split, ratio }) => {
                self.panes.resize(split, ratio);
                Task::none()
            }
            Message::PaneDragged(pane_grid::DragEvent::Dropped { pane, target }) => {
                self.panes.drop(pane, target);
                Task::none()
            }
            Message::PaneDragged(_) => Task::none(),
            Message::PaneClicked(pane) => {
                self.focused_pane = pane;
                Task::none()
            }
            Message::SaveActive => {
                let Some(tab_id) = self.active_tab_id(self.focused_pane) else {
                    return Task::none();
                };
                self.save_tab(tab_id)
            }
            Message::SavePathChosen(tab_id, Some(path)) => {
                if let Some(Tab::Notebook(nb)) = self.tabs.get_mut(&tab_id) {
                    nb.doc.path = Some(path);
                }
                self.save_tab(tab_id)
            }
            Message::SavePathChosen(_, None) => Task::none(),
            Message::Saved(_, Ok(())) => {
                self.status_line = "saved".to_string();
                Task::none()
            }
            Message::Saved(_, Err(e)) => {
                self.status_line = format!("save failed: {e}");
                Task::none()
            }
        }
    }

    fn on_sidebar(&mut self, event: sidebar::Event) -> Task<Message> {
        match event {
            sidebar::Event::Navigate(path) => Task::perform(sidebar::read_dir(path), |r| {
                Message::Sidebar(sidebar::Event::Loaded(r))
            }),
            sidebar::Event::Up => {
                let parent = sidebar::parent_of(&self.browser.cwd);
                Task::perform(sidebar::read_dir(parent), |r| {
                    Message::Sidebar(sidebar::Event::Loaded(r))
                })
            }
            sidebar::Event::Refresh => {
                let cwd = self.browser.cwd.clone();
                Task::perform(sidebar::read_dir(cwd), |r| {
                    Message::Sidebar(sidebar::Event::Loaded(r))
                })
            }
            sidebar::Event::Loaded(listing) => {
                self.browser.apply(listing);
                Task::none()
            }
            sidebar::Event::FilterChanged(filter) => {
                self.browser.filter = filter;
                Task::none()
            }
            sidebar::Event::Select(i) => {
                self.browser.selected = Some(i);
                Task::none()
            }
            sidebar::Event::OpenNotebook(path) => self.open_notebook(path),
        }
    }

    fn on_launcher(&mut self, event: launcher::Event) -> Task<Message> {
        match event {
            launcher::Event::NewNotebook(kernel_name) => self.new_notebook(kernel_name),
            launcher::Event::NewConsole(kernel_name) => {
                let language = self
                    .specs
                    .iter()
                    .find(|s| s.kernel_name == kernel_name)
                    .map(|s| language_token_for(&s.kernelspec.language))
                    .unwrap_or_else(|| "python".to_string());
                let id = self.alloc_tab(Tab::Console(ConsoleTab {
                    entries: Vec::new(),
                    input: text_editor::Content::new(),
                    kernel: KernelState::Launching,
                    pending: HashMap::new(),
                    language,
                    kernel_label: "starting...".to_string(),
                }));
                self.add_tab_to_focused(id);
                Task::stream(kernel_events(Some(kernel_name)))
                    .map(move |msg| Message::Kernel(id, msg))
            }
        }
    }

    fn on_console(&mut self, tab_id: TabId, event: console_view::Event) -> Task<Message> {
        let Some(Tab::Console(console)) = self.tabs.get_mut(&tab_id) else {
            return Task::none();
        };
        match event {
            console_view::Event::InputAction(action) => {
                console.input.perform(action);
                Task::none()
            }
            console_view::Event::LinkClicked(uri) => {
                let _ = open::that(uri.to_string());
                Task::none()
            }
            console_view::Event::Submit => {
                let code = console.input.text();
                if code.trim().is_empty() {
                    return Task::none();
                }
                let KernelState::Ready { handle, .. } = &console.kernel else {
                    self.status_line = "kernel not ready".to_string();
                    return Task::none();
                };
                let exec: JupyterMessage = ExecuteRequest::new(code.clone()).into();
                console
                    .pending
                    .insert(exec.header.msg_id.clone(), console.entries.len());
                console.entries.push(console_view::ConsoleEntry {
                    execution_count: None,
                    source: code.trim_end().to_string(),
                    outputs: Vec::new(),
                    running: true,
                });
                console.input = text_editor::Content::new();
                let commands = handle.commands.clone();
                Task::future(async move {
                    let _ = commands.send(KernelCommand::Shell(exec)).await;
                })
                .discard()
            }
        }
    }

    fn on_command_key(&mut self, key: &str) -> Task<Message> {
        let Some(tab_id) = self.active_tab_id(self.focused_pane) else {
            return Task::none();
        };
        let Some(Tab::Notebook(nb)) = self.tabs.get_mut(&tab_id) else {
            return Task::none();
        };
        let was_pending_delete = std::mem::take(&mut self.pending_delete);
        let event = match key {
            "a" => Some(notebook_view::Event::AddCellAbove),
            "b" => Some(notebook_view::Event::AddCellBelow),
            "m" => Some(notebook_view::Event::SetCellType(
                notebook_view::CellTypeChoice::Markdown,
            )),
            "y" => Some(notebook_view::Event::SetCellType(
                notebook_view::CellTypeChoice::Code,
            )),
            "d" => {
                if was_pending_delete {
                    Some(notebook_view::Event::DeleteCell)
                } else {
                    self.pending_delete = true;
                    None
                }
            }
            _ => None,
        };
        let _ = nb;
        match event {
            Some(event) => self.on_notebook(tab_id, event),
            None => Task::none(),
        }
    }

    fn on_notebook(&mut self, tab_id: TabId, event: notebook_view::Event) -> Task<Message> {
        let Some(Tab::Notebook(nb)) = self.tabs.get_mut(&tab_id) else {
            return Task::none();
        };
        match event {
            notebook_view::Event::CellAction(index, action) => {
                if let Some(cell) = nb.doc.cells.get_mut(index) {
                    let is_edit = action.is_edit();
                    cell.source.perform(action);
                    if is_edit {
                        nb.doc.dirty = true;
                    }
                }
                Task::none()
            }
            notebook_view::Event::RunCell(index) => nb.run_cell(index, &mut self.status_line),
            notebook_view::Event::EditMarkdown(index) => {
                if let Some(cell) = nb.doc.cells.get_mut(index) {
                    if let CellKind::Markdown { editing, .. } = &mut cell.kind {
                        *editing = true;
                    }
                }
                Task::none()
            }
            notebook_view::Event::LinkClicked(uri) => {
                let _ = open::that(uri.to_string());
                Task::none()
            }
            notebook_view::Event::Save => self.save_tab(tab_id),
            notebook_view::Event::Interrupt => {
                if let KernelState::Ready { handle, .. } = &nb.kernel {
                    let msg: JupyterMessage = InterruptRequest {}.into();
                    let _ = handle.commands.try_send(KernelCommand::Control(msg));
                }
                Task::none()
            }
            notebook_view::Event::Restart => {
                if let KernelState::Ready { handle, .. } = &nb.kernel {
                    let _ = handle.commands.try_send(KernelCommand::Shutdown);
                }
                nb.kernel = KernelState::Launching;
                nb.pending.clear();
                nb.queue.clear();
                for cell in &mut nb.doc.cells {
                    cell.running = false;
                }
                let preferred = nb.doc.metadata.kernelspec.as_ref().map(|k| k.name.clone());
                Task::stream(kernel_events(preferred))
                    .map(move |msg| Message::Kernel(tab_id, msg))
            }
            notebook_view::Event::SelectCell(index) => {
                nb.selected = index.min(nb.doc.cells.len().saturating_sub(1));
                Task::none()
            }
            notebook_view::Event::AddCellAbove => {
                let at = nb.selected.min(nb.doc.cells.len());
                nb.doc.cells.insert(at, model::new_code_cell());
                nb.selected = at;
                nb.doc.dirty = true;
                Task::none()
            }
            notebook_view::Event::AddCellBelow => {
                let at = (nb.selected + 1).min(nb.doc.cells.len());
                nb.doc.cells.insert(at, model::new_code_cell());
                nb.selected = at;
                nb.doc.dirty = true;
                Task::none()
            }
            notebook_view::Event::DeleteCell => {
                if nb.doc.cells.len() > 1 && nb.selected < nb.doc.cells.len() {
                    nb.doc.cells.remove(nb.selected);
                    nb.selected = nb.selected.min(nb.doc.cells.len() - 1);
                    nb.doc.dirty = true;
                }
                Task::none()
            }
            notebook_view::Event::MoveCellUp => {
                if nb.selected > 0 && nb.selected < nb.doc.cells.len() {
                    nb.doc.cells.swap(nb.selected, nb.selected - 1);
                    nb.selected -= 1;
                    nb.doc.dirty = true;
                }
                Task::none()
            }
            notebook_view::Event::MoveCellDown => {
                if nb.selected + 1 < nb.doc.cells.len() {
                    nb.doc.cells.swap(nb.selected, nb.selected + 1);
                    nb.selected += 1;
                    nb.doc.dirty = true;
                }
                Task::none()
            }
            notebook_view::Event::SetCellType(choice) => {
                if let Some(cell) = nb.doc.cells.get_mut(nb.selected) {
                    model::set_cell_kind(cell, match choice {
                        notebook_view::CellTypeChoice::Code => model::CellKindTag::Code,
                        notebook_view::CellTypeChoice::Markdown => model::CellKindTag::Markdown,
                        notebook_view::CellTypeChoice::Raw => model::CellKindTag::Raw,
                    });
                    nb.doc.dirty = true;
                }
                Task::none()
            }
            notebook_view::Event::RunAll => {
                nb.queue = nb
                    .doc
                    .cells
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.is_code())
                    .map(|(i, _)| i)
                    .collect();
                // Render all markdown cells too, like JupyterLab's Run All.
                for cell in &mut nb.doc.cells {
                    if let CellKind::Markdown { rendered, editing } = &mut cell.kind {
                        *rendered =
                            iced::widget::markdown::Content::parse(&cell.source.text());
                        *editing = false;
                    }
                }
                match nb.queue.pop_front() {
                    Some(first) => nb.run_cell(first, &mut self.status_line),
                    None => Task::none(),
                }
            }
        }
    }

    fn on_kernel_msg(&mut self, tab_id: TabId, msg: KernelMsg) -> Task<Message> {
        match self.tabs.get_mut(&tab_id) {
            Some(Tab::Notebook(nb)) => {
                match msg {
                    KernelMsg::Ready(handle) => {
                        nb.kernel_label = handle
                            .connection_info
                            .kernel_name
                            .clone()
                            .unwrap_or_else(|| "kernel".to_string());
                        nb.kernel = KernelState::Ready {
                            handle,
                            busy: false,
                        };
                    }
                    KernelMsg::Failed(e) => {
                        nb.kernel = KernelState::Dead { reason: e };
                    }
                    KernelMsg::Event(KernelEvent::Exited(code)) => {
                        // A restart replaces the state with Launching first;
                        // only a Ready/Dead kernel exiting is terminal.
                        if !matches!(nb.kernel, KernelState::Launching) {
                            nb.kernel = KernelState::Dead {
                                reason: format!("exited (code {code:?})"),
                            };
                        }
                    }
                    KernelMsg::Event(KernelEvent::ShellReply(reply)) => {
                        if let JupyterMessageContent::ExecuteReply(r) = &reply.content {
                            let parent =
                                reply.parent_header.as_ref().map(|h| h.msg_id.clone());
                            if let Some(index) =
                                parent.and_then(|id| nb.pending.get(&id).copied())
                            {
                                if let Some(cell) = nb.doc.cells.get_mut(index) {
                                    cell.execution_count =
                                        Some(r.execution_count.value() as i32);
                                }
                            }
                        }
                    }
                    KernelMsg::Event(KernelEvent::IoPub(msg)) => {
                        nb.on_iopub(msg);
                        // Drive the Run All queue: dispatch the next cell once
                        // the previous execution fully completed.
                        if nb.pending.is_empty() {
                            if let Some(next) = nb.queue.pop_front() {
                                return nb.run_cell(next, &mut self.status_line);
                            }
                        }
                    }
                }
                Task::none()
            }
            Some(Tab::Console(console)) => {
                console.on_kernel_msg(msg);
                Task::none()
            }
            _ => Task::none(),
        }
    }

    // ------------------------------------------------------------------
    // Tab management
    // ------------------------------------------------------------------

    fn alloc_tab(&mut self, tab: Tab) -> TabId {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.tabs.insert(id, tab);
        id
    }

    fn active_tab_id(&self, pane: pane_grid::Pane) -> Option<TabId> {
        let state = self.panes.get(pane)?;
        state.tabs.get(state.active).copied()
    }

    fn add_tab_to_focused(&mut self, id: TabId) {
        let pane = self.focused_pane;
        if let Some(state) = self.panes.get_mut(pane) {
            state.tabs.push(id);
            state.active = state.tabs.len() - 1;
        }
    }

    fn open_notebook(&mut self, path: PathBuf) -> Task<Message> {
        // Focus the tab if this notebook is already open.
        let already_open = self.tabs.iter().find_map(|(id, tab)| match tab {
            Tab::Notebook(nb) if nb.doc.path.as_deref() == Some(path.as_path()) => Some(*id),
            _ => None,
        });
        if let Some(id) = already_open {
            self.focus_tab(id);
            return Task::none();
        }

        match model::load(&path) {
            Ok(doc) => {
                let preferred = doc.metadata.kernelspec.as_ref().map(|k| k.name.clone());
                let language = language_token(&doc);
                let id = self.alloc_tab(Tab::Notebook(NotebookTab::new(doc, language)));
                self.add_tab_to_focused(id);
                Task::stream(kernel_events(preferred)).map(move |msg| Message::Kernel(id, msg))
            }
            Err(e) => {
                self.status_line = format!("failed to open {}: {e:#}", path.display());
                Task::none()
            }
        }
    }

    fn new_notebook(&mut self, kernel_name: String) -> Task<Message> {
        let spec = self.specs.iter().find(|s| s.kernel_name == kernel_name);
        let mut metadata = nbformat::v4::Metadata::default();
        if let Some(spec) = spec {
            metadata.kernelspec = Some(nbformat::v4::KernelSpec {
                display_name: spec.kernelspec.display_name.clone(),
                name: spec.kernel_name.clone(),
                language: Some(spec.kernelspec.language.clone()),
                additional: HashMap::new(),
            });
        }
        let doc = NotebookDoc {
            path: None,
            metadata,
            nbformat_minor: 5,
            cells: vec![model::new_code_cell()],
            dirty: true,
        };
        let language = language_token(&doc);
        let id = self.alloc_tab(Tab::Notebook(NotebookTab::new(doc, language)));
        self.add_tab_to_focused(id);
        Task::stream(kernel_events(Some(kernel_name))).map(move |msg| Message::Kernel(id, msg))
    }

    fn focus_tab(&mut self, id: TabId) {
        let panes: Vec<pane_grid::Pane> = self.panes.iter().map(|(p, _)| *p).collect();
        for pane in panes {
            if let Some(state) = self.panes.get_mut(pane) {
                if let Some(pos) = state.tabs.iter().position(|t| *t == id) {
                    state.active = pos;
                    self.focused_pane = pane;
                    return;
                }
            }
        }
    }

    fn close_tab(&mut self, pane: pane_grid::Pane, index: usize) -> Task<Message> {
        let Some(state) = self.panes.get_mut(pane) else {
            return Task::none();
        };
        if index >= state.tabs.len() {
            return Task::none();
        }
        let id = state.tabs.remove(index);
        if state.active >= state.tabs.len() {
            state.active = state.tabs.len().saturating_sub(1);
        }
        let empty = state.tabs.is_empty();

        match self.tabs.remove(&id) {
            Some(Tab::Notebook(nb)) => {
                if let KernelState::Ready { handle, .. } = &nb.kernel {
                    let _ = handle.commands.try_send(KernelCommand::Shutdown);
                }
            }
            Some(Tab::Console(console)) => {
                if let KernelState::Ready { handle, .. } = &console.kernel {
                    let _ = handle.commands.try_send(KernelCommand::Shutdown);
                }
            }
            _ => {}
        }

        if empty {
            // Keep at least one pane alive; the last pane gets a Launcher.
            if self.panes.len() > 1 {
                if let Some((_, sibling)) = self.panes.close(pane) {
                    self.focused_pane = sibling;
                }
            } else {
                let id = self.alloc_tab(Tab::Launcher);
                if let Some(state) = self.panes.get_mut(pane) {
                    state.tabs.push(id);
                    state.active = 0;
                }
            }
        }
        Task::none()
    }

    fn save_tab(&mut self, tab_id: TabId) -> Task<Message> {
        let Some(Tab::Notebook(nb)) = self.tabs.get_mut(&tab_id) else {
            return Task::none();
        };
        let Some(path) = nb.doc.path.clone() else {
            let cwd = self.browser.cwd.clone();
            return Task::perform(
                async move {
                    rfd::AsyncFileDialog::new()
                        .add_filter("Jupyter Notebook", &["ipynb"])
                        .set_directory(cwd)
                        .set_file_name("Untitled.ipynb")
                        .save_file()
                        .await
                        .map(|f| f.path().to_path_buf())
                },
                move |path| Message::SavePathChosen(tab_id, path),
            );
        };
        match model::save(&nb.doc) {
            Ok(json) => {
                nb.doc.dirty = false;
                Task::perform(
                    async move {
                        tokio::fs::write(&path, json)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |r| Message::Saved(tab_id, r),
                )
            }
            Err(e) => {
                self.status_line = format!("serialize failed: {e:#}");
                Task::none()
            }
        }
    }

    // ------------------------------------------------------------------
    // View
    // ------------------------------------------------------------------

    fn subscription(&self) -> Subscription<Message> {
        iced::event::listen_with(|event, status, _window| {
            if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Character(c),
                modifiers,
                ..
            }) = &event
            {
                if modifiers.command() && c.as_str() == "s" {
                    return Some(Message::SaveActive);
                }
                // Command-mode keys only apply when no widget (e.g. a focused
                // cell editor) consumed the key press.
                if status == iced::event::Status::Ignored
                    && !modifiers.command()
                    && !modifiers.control()
                    && !modifiers.alt()
                {
                    return Some(Message::CommandKey(c.to_string()));
                }
            }
            None
        })
    }

    fn view(&self) -> Element<'_, Message> {
        let menu_bar = container(
            row(["File", "Edit", "View", "Run", "Kernel", "Tabs", "Settings", "Help"]
                .iter()
                .map(|label| container(text(*label).size(13)).padding([4, 10]).into()))
            .spacing(2),
        )
        .width(Fill)
        .style(container::bordered_box);

        let panes = pane_grid(&self.panes, |pane, state, _maximized| {
            let tab_strip = self.view_tab_strip(pane, state);
            let body: Element<'_, Message> = match state
                .tabs
                .get(state.active)
                .and_then(|id| self.tabs.get(id).map(|t| (*id, t)))
            {
                Some((id, Tab::Launcher)) => {
                    let _ = id;
                    launcher::view(&self.specs).map(Message::Launcher)
                }
                Some((id, Tab::Notebook(nb))) => {
                    let indicator = notebook_view::KernelIndicator {
                        label: match &nb.kernel {
                            KernelState::Launching => "starting...",
                            KernelState::Ready { .. } => &nb.kernel_label,
                            KernelState::Dead { .. } => "dead",
                        },
                        busy: match &nb.kernel {
                            KernelState::Ready { busy, .. } => Some(*busy),
                            _ => None,
                        },
                    };
                    notebook_view::view(&nb.doc, &nb.language, indicator, nb.selected)
                        .map(move |e| Message::Notebook(id, e))
                }
                Some((id, Tab::Console(console))) => {
                    let (label, busy) = match &console.kernel {
                        KernelState::Launching => ("starting...", None),
                        KernelState::Ready { busy, .. } => {
                            (console.kernel_label.as_str(), Some(*busy))
                        }
                        KernelState::Dead { .. } => ("dead", None),
                    };
                    console_view::view(
                        &console.entries,
                        &console.input,
                        &console.language,
                        label,
                        busy,
                    )
                    .map(move |e| Message::Console(id, e))
                }
                None => text("").into(),
            };

            pane_grid::Content::new(body)
                .title_bar(pane_grid::TitleBar::new(tab_strip))
        })
        .spacing(4)
        .on_click(Message::PaneClicked)
        .on_resize(8, Message::PaneResized)
        .on_drag(Message::PaneDragged);

        let workspace = row![
            self.browser.view().map(Message::Sidebar),
            container(panes).width(Fill).height(Fill).padding(4),
        ];

        let status_bar = container(text(&self.status_line).size(12))
            .width(Fill)
            .padding([2, 10]);

        column![menu_bar, workspace, status_bar].into()
    }

    fn view_tab_strip<'a>(
        &'a self,
        pane: pane_grid::Pane,
        state: &'a PaneTabs,
    ) -> Element<'a, Message> {
        let mut strip = row![].spacing(2);
        for (i, id) in state.tabs.iter().enumerate() {
            let title = match self.tabs.get(id) {
                Some(Tab::Launcher) => "Launcher".to_string(),
                Some(Tab::Console(console)) => format!("Console ({})", console.kernel_label),
                Some(Tab::Notebook(nb)) => {
                    let name = nb
                        .doc
                        .path
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Untitled.ipynb".to_string());
                    if nb.doc.dirty {
                        format!("● {name}")
                    } else {
                        name
                    }
                }
                None => "?".to_string(),
            };
            let is_active = i == state.active;
            strip = strip.push(
                row![
                    button(text(title).size(12))
                        .padding([3, 8])
                        .style(if is_active {
                            button::primary
                        } else {
                            button::text
                        })
                        .on_press(Message::SelectTab(pane, i)),
                    button(text("✕").size(11))
                        .padding([3, 4])
                        .style(button::text)
                        .on_press(Message::CloseTab(pane, i)),
                ]
                .spacing(0),
            );
        }
        strip = strip.push(
            button(text("+").size(13))
                .padding([2, 8])
                .style(button::text)
                .on_press(Message::NewLauncher(pane)),
        );
        strip = strip.push(
            button(text("⫲").size(12))
                .padding([2, 8])
                .style(button::text)
                .on_press(Message::SplitPane(pane)),
        );
        strip.into()
    }
}

impl NotebookTab {
    fn run_cell(&mut self, index: usize, status_line: &mut String) -> Task<Message> {
        // "Running" a markdown cell renders it, like in JupyterLab.
        if let Some(cell) = self.doc.cells.get_mut(index) {
            if let CellKind::Markdown { rendered, editing } = &mut cell.kind {
                *rendered = iced::widget::markdown::Content::parse(&cell.source.text());
                *editing = false;
                return Task::none();
            }
        }

        let KernelState::Ready { handle, .. } = &self.kernel else {
            *status_line = "kernel not ready".to_string();
            return Task::none();
        };
        let Some(cell) = self.doc.cells.get_mut(index) else {
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
        self.doc.dirty = true;

        let commands = handle.commands.clone();
        Task::future(async move {
            let _ = commands.send(KernelCommand::Shell(exec)).await;
        })
        .discard()
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
                    if let Some(cell) = self.doc.cells.get_mut(index) {
                        cell.running = false;
                    }
                }
            }
            return;
        }

        let Some(index) = cell_index else { return };
        let Some(cell) = self.doc.cells.get_mut(index) else {
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
}

impl ConsoleTab {
    fn on_kernel_msg(&mut self, msg: KernelMsg) {
        match msg {
            KernelMsg::Ready(handle) => {
                self.kernel_label = handle
                    .connection_info
                    .kernel_name
                    .clone()
                    .unwrap_or_else(|| "kernel".to_string());
                self.kernel = KernelState::Ready {
                    handle,
                    busy: false,
                };
            }
            KernelMsg::Failed(e) => {
                self.kernel = KernelState::Dead { reason: e };
            }
            KernelMsg::Event(KernelEvent::Exited(code)) => {
                if !matches!(self.kernel, KernelState::Launching) {
                    self.kernel = KernelState::Dead {
                        reason: format!("exited (code {code:?})"),
                    };
                }
            }
            KernelMsg::Event(KernelEvent::ShellReply(reply)) => {
                if let JupyterMessageContent::ExecuteReply(r) = &reply.content {
                    let parent = reply.parent_header.as_ref().map(|h| h.msg_id.clone());
                    if let Some(index) = parent.and_then(|id| self.pending.get(&id).copied()) {
                        if let Some(entry) = self.entries.get_mut(index) {
                            entry.execution_count = Some(r.execution_count.value() as i32);
                        }
                    }
                }
            }
            KernelMsg::Event(KernelEvent::IoPub(msg)) => {
                let parent_id = msg.parent_header.as_ref().map(|h| h.msg_id.clone());
                let entry_index =
                    parent_id.as_ref().and_then(|id| self.pending.get(id)).copied();

                if let JupyterMessageContent::Status(s) = &msg.content {
                    if let KernelState::Ready { busy, .. } = &mut self.kernel {
                        *busy = s.execution_state == ExecutionState::Busy;
                    }
                    if s.execution_state == ExecutionState::Idle {
                        if let (Some(index), Some(id)) = (entry_index, parent_id.as_ref()) {
                            self.pending.remove(id);
                            if let Some(entry) = self.entries.get_mut(index) {
                                entry.running = false;
                            }
                        }
                    }
                    return;
                }

                let Some(entry) = entry_index.and_then(|i| self.entries.get_mut(i)) else {
                    return;
                };
                match msg.content {
                    JupyterMessageContent::StreamContent(s) => {
                        let name = match s.name {
                            jupyter_protocol::Stdio::Stdout => "stdout",
                            jupyter_protocol::Stdio::Stderr => "stderr",
                        };
                        if let Some(CellOutput::Stream { name: last, text }) =
                            entry.outputs.last_mut()
                        {
                            if last == name {
                                text.push_str(&s.text);
                                return;
                            }
                        }
                        entry.outputs.push(CellOutput::Stream {
                            name: name.to_string(),
                            text: s.text,
                        });
                    }
                    JupyterMessageContent::ExecuteInput(input) => {
                        entry.execution_count = Some(input.execution_count.value() as i32);
                    }
                    JupyterMessageContent::ExecuteResult(r) => {
                        entry.outputs.push(CellOutput::Data {
                            rendered: render::prepare(&r.data),
                            media: r.data,
                            execution_count: Some(r.execution_count),
                        });
                    }
                    JupyterMessageContent::DisplayData(d) => {
                        entry.outputs.push(CellOutput::Data {
                            rendered: render::prepare(&d.data),
                            media: d.data,
                            execution_count: None,
                        });
                    }
                    JupyterMessageContent::ErrorOutput(e) => {
                        entry.outputs.push(CellOutput::Error {
                            ename: e.ename,
                            evalue: e.evalue,
                            traceback: e.traceback,
                        });
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Map a notebook's language to a syntect token iced's highlighter accepts.
fn language_token(doc: &NotebookDoc) -> String {
    let language = doc
        .metadata
        .language_info
        .as_ref()
        .map(|l| l.name.clone())
        .or_else(|| {
            doc.metadata
                .kernelspec
                .as_ref()
                .and_then(|k| k.language.clone())
        })
        .unwrap_or_else(|| "python".to_string());
    language_token_for(&language)
}

fn language_token_for(language: &str) -> String {
    match language.to_lowercase().as_str() {
        "python" => "python",
        // Mojo is a Python superset; the default syntax set has no Mojo grammar.
        "mojo" => "python",
        "julia" => "julia",
        "rust" => "rust",
        "r" => "r",
        "markdown" => "markdown",
        _ => "txt",
    }
    .to_string()
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

// Keep the text_editor type in scope for Message (used via notebook_view).
#[allow(unused_imports)]
use text_editor as _text_editor_used_by_events;
