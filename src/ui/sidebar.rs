//! Left sidebar: file browser panel, JupyterLab style.

use std::path::{Path, PathBuf};

use iced::widget::{button, column, container, mouse_area, row, scrollable, text, text_input};
use iced::{Element, Fill};

#[derive(Debug, Clone)]
pub enum Event {
    Navigate(PathBuf),
    OpenNotebook(PathBuf),
    Refresh,
    Up,
    FilterChanged(String),
    Loaded(Result<Listing, String>),
    Select(usize),
}

#[derive(Debug, Clone)]
pub struct Listing {
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

pub struct FileBrowser {
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub filter: String,
    pub selected: Option<usize>,
    pub error: Option<String>,
}

impl FileBrowser {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            entries: Vec::new(),
            filter: String::new(),
            selected: None,
            error: None,
        }
    }

    pub fn apply(&mut self, listing: Result<Listing, String>) {
        match listing {
            Ok(listing) => {
                self.cwd = listing.dir;
                self.entries = listing.entries;
                self.selected = None;
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn view(&self) -> Element<'_, Event> {
        let toolbar = row![
            button(text("↑").size(14))
                .style(crate::ui::style::toolbar_button)
                .on_press(Event::Up)
                .padding([4, 8]),
            button(text("⟳").size(14))
                .style(crate::ui::style::toolbar_button)
                .on_press(Event::Refresh)
                .padding([4, 8]),
        ]
        .spacing(4);

        let filter = text_input("Filter files by name", &self.filter)
            .on_input(Event::FilterChanged)
            .size(13)
            .padding(6);

        let cwd_label = text(format!(
            "/ {}",
            self.cwd
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ))
        .size(12);

        let filter_lower = self.filter.to_lowercase();
        let rows = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| filter_lower.is_empty() || e.name.to_lowercase().contains(&filter_lower))
            .map(|(i, entry)| {
                let icon = if entry.is_dir {
                    "📁"
                } else if entry.name.ends_with(".ipynb") {
                    "📔"
                } else {
                    "📄"
                };
                let label = row![text(icon).size(13), text(&entry.name).size(13)].spacing(6);

                let target = if entry.is_dir {
                    Event::Navigate(entry.path.clone())
                } else if entry.name.ends_with(".ipynb") {
                    Event::OpenNotebook(entry.path.clone())
                } else {
                    Event::Select(i)
                };

                let styled = container(label).padding([3, 6]).width(Fill).style(
                    if self.selected == Some(i) {
                        container::secondary
                    } else {
                        container::transparent
                    },
                );

                mouse_area(styled)
                    .on_press(Event::Select(i))
                    .on_double_click(target)
                    .into()
            });

        let mut content = column![toolbar, filter, cwd_label].spacing(8);
        if let Some(err) = &self.error {
            content = content.push(text(err).size(12));
        }
        content = content.push(scrollable(column(rows)).height(Fill));

        container(content.padding(10))
            .width(260)
            .height(Fill)
            .style(container::bordered_box)
            .into()
    }
}

pub async fn read_dir(dir: PathBuf) -> Result<Listing, String> {
    let canonical = dir.canonicalize().map_err(|e| e.to_string())?;
    let mut read = tokio::fs::read_dir(&canonical).await.map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    while let Ok(Some(item)) = read.next_entry().await {
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let is_dir = item.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
        entries.push(Entry {
            name,
            path: item.path(),
            is_dir,
        });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(Listing {
        dir: canonical,
        entries,
    })
}

pub fn parent_of(path: &Path) -> PathBuf {
    path.parent().map(Path::to_path_buf).unwrap_or_else(|| path.to_path_buf())
}
