//! Menu bar with dropdown menus, built from a stack overlay (iced has no
//! native menu widget).

use iced::widget::{button, column, container, row, text, Space};
use iced::{Element, Fill};

pub const MENU_BAR_HEIGHT: f32 = 30.0;
pub const MENU_BUTTON_WIDTH: f32 = 78.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuId {
    File,
    Edit,
    View,
    Run,
    Kernel,
    Tabs,
    Settings,
    Help,
}

pub const MENUS: [(MenuId, &str); 8] = [
    (MenuId::File, "File"),
    (MenuId::Edit, "Edit"),
    (MenuId::View, "View"),
    (MenuId::Run, "Run"),
    (MenuId::Kernel, "Kernel"),
    (MenuId::Tabs, "Tabs"),
    (MenuId::Settings, "Settings"),
    (MenuId::Help, "Help"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    NewNotebook,
    NewLauncherTab,
    OpenFile,
    Save,
    SaveAs,
    CloseTab,
    AddCellAbove,
    AddCellBelow,
    DeleteCell,
    MoveCellUp,
    MoveCellDown,
    CellToCode,
    CellToMarkdown,
    ToggleSidebar,
    LightTheme,
    DarkTheme,
    RunCell,
    RunAll,
    Interrupt,
    Restart,
    SplitPane,
    NextTab,
    PreviousTab,
    JupyterDocs,
    About,
}

#[derive(Debug, Clone)]
pub enum Event {
    Toggle(MenuId),
    Action(MenuAction),
    Close,
}

fn items(menu: MenuId) -> Vec<(&'static str, MenuAction)> {
    use MenuAction::*;
    match menu {
        MenuId::File => vec![
            ("New Notebook", NewNotebook),
            ("New Launcher", NewLauncherTab),
            ("Open...", OpenFile),
            ("Save", Save),
            ("Save As...", SaveAs),
            ("Close Tab", CloseTab),
        ],
        MenuId::Edit => vec![
            ("Insert Cell Above", AddCellAbove),
            ("Insert Cell Below", AddCellBelow),
            ("Delete Cell", DeleteCell),
            ("Move Cell Up", MoveCellUp),
            ("Move Cell Down", MoveCellDown),
            ("Change to Code", CellToCode),
            ("Change to Markdown", CellToMarkdown),
        ],
        MenuId::View => vec![("Toggle Sidebar", ToggleSidebar)],
        MenuId::Run => vec![("Run Selected Cell", RunCell), ("Run All Cells", RunAll)],
        MenuId::Kernel => vec![
            ("Interrupt Kernel", Interrupt),
            ("Restart Kernel", Restart),
        ],
        MenuId::Tabs => vec![
            ("Split Pane", SplitPane),
            ("Next Tab", NextTab),
            ("Previous Tab", PreviousTab),
        ],
        MenuId::Settings => vec![("Light Theme", LightTheme), ("Dark Theme", DarkTheme)],
        MenuId::Help => vec![("Jupyter Documentation", JupyterDocs), ("About RustLab", About)],
    }
}

pub fn bar(open: Option<MenuId>) -> Element<'static, Event> {
    container(
        row(MENUS.iter().map(|(id, label)| {
            let is_open = open == Some(*id);
            button(text(*label).size(13).center())
                .width(MENU_BUTTON_WIDTH)
                .padding([5, 0])
                .style(if is_open { button::secondary } else { button::text })
                .on_press(Event::Toggle(*id))
                .into()
        }))
        .spacing(0),
    )
    .width(Fill)
    .height(MENU_BAR_HEIGHT)
    .style(container::bordered_box)
    .into()
}

/// The dropdown overlay: a click-away backdrop plus the open menu's panel,
/// positioned under its menu-bar button. Layer this over the workspace with
/// `stack`, offset below the menu bar.
pub fn dropdown(menu: MenuId) -> Element<'static, Event> {
    let index = MENUS.iter().position(|(id, _)| *id == menu).unwrap_or(0);

    let panel = container(column(items(menu).into_iter().map(|(label, action)| {
        button(text(label).size(13))
            .width(Fill)
            .padding([5, 12])
            .style(button::text)
            .on_press(Event::Action(action))
            .into()
    })))
    .width(230)
    .style(|theme: &iced::Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(palette.background.base.color.into()),
            border: iced::Border {
                color: palette.background.strong.color,
                width: 1.0,
                radius: 4.0.into(),
            },
            shadow: iced::Shadow {
                color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.25),
                offset: iced::Vector::new(0.0, 2.0),
                blur_radius: 8.0,
            },
            ..container::Style::default()
        }
    });

    let backdrop = iced::widget::mouse_area(
        container(Space::new().width(Fill).height(Fill))
            .width(Fill)
            .height(Fill),
    )
    .on_press(Event::Close);

    iced::widget::stack![
        backdrop,
        column![
            Space::new().height(MENU_BAR_HEIGHT),
            row![
                Space::new().width(index as f32 * MENU_BUTTON_WIDTH),
                panel
            ],
        ],
    ]
    .width(Fill)
    .height(Fill)
    .into()
}
