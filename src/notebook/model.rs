//! In-memory notebook document: the app-side cell model and its mapping
//! to/from the on-disk nbformat v4 representation.

use std::path::PathBuf;

use anyhow::{Context, Result};
use iced::widget::{markdown, text_editor};
use jupyter_protocol::{ExecutionCount, Media};
use nbformat::v4;

use crate::output::render::{self, Rendered};

pub struct NotebookDoc {
    pub path: Option<PathBuf>,
    pub metadata: v4::Metadata,
    pub nbformat_minor: i32,
    pub cells: Vec<CellState>,
    pub dirty: bool,
}

pub enum CellKind {
    Code,
    Markdown {
        rendered: markdown::Content,
        editing: bool,
    },
    Raw,
}

pub struct CellState {
    pub id: v4::CellId,
    pub kind: CellKind,
    pub source: text_editor::Content,
    pub outputs: Vec<CellOutput>,
    pub execution_count: Option<i32>,
    pub metadata: v4::CellMetadata,
    pub running: bool,
}

/// A cell output, unified across live kernel messages and saved notebooks.
#[derive(Debug)]
pub enum CellOutput {
    Stream {
        name: String,
        text: String,
    },
    /// display_data, or execute_result when `execution_count` is set.
    Data {
        media: Media,
        execution_count: Option<ExecutionCount>,
        rendered: Rendered,
    },
    Error {
        ename: String,
        evalue: String,
        traceback: Vec<String>,
    },
}

impl CellState {
    pub fn source_text(&self) -> String {
        self.source.text()
    }

    pub fn is_code(&self) -> bool {
        matches!(self.kind, CellKind::Code)
    }

    /// Append a stream chunk, merging with a trailing stream output of the
    /// same name the way JupyterLab does.
    pub fn push_stream(&mut self, name: &str, chunk: &str) {
        if let Some(CellOutput::Stream { name: last, text }) = self.outputs.last_mut() {
            if last == name {
                text.push_str(chunk);
                return;
            }
        }
        self.outputs.push(CellOutput::Stream {
            name: name.to_string(),
            text: chunk.to_string(),
        });
    }
}

pub fn load(path: &std::path::Path) -> Result<NotebookDoc> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let notebook = match nbformat::parse_notebook(&json)? {
        nbformat::Notebook::V4(nb) => nb,
        nbformat::Notebook::V4QuirksMode(quirked) => quirked.repair(),
        nbformat::Notebook::Legacy(legacy) => nbformat::upgrade_legacy_notebook(legacy)?,
        nbformat::Notebook::V3(v3) => nbformat::upgrade_v3_notebook(v3)?,
        other => anyhow::bail!("unsupported notebook variant: {other:?}"),
    };

    let cells = notebook.cells.into_iter().map(cell_from_nbformat).collect();
    Ok(NotebookDoc {
        path: Some(path.to_path_buf()),
        metadata: notebook.metadata,
        nbformat_minor: notebook.nbformat_minor,
        cells,
        dirty: false,
    })
}

pub fn save(doc: &NotebookDoc) -> Result<String> {
    let notebook = v4::Notebook {
        metadata: doc.metadata.clone(),
        nbformat: 4,
        nbformat_minor: doc.nbformat_minor.max(5),
        cells: doc.cells.iter().map(cell_to_nbformat).collect(),
    };
    Ok(nbformat::serialize_notebook(&nbformat::Notebook::V4(
        notebook,
    ))?)
}

fn cell_from_nbformat(cell: v4::Cell) -> CellState {
    match cell {
        v4::Cell::Code {
            id,
            metadata,
            execution_count,
            source,
            outputs,
        } => CellState {
            id,
            kind: CellKind::Code,
            source: text_editor::Content::with_text(&source.concat()),
            outputs: outputs.into_iter().map(output_from_nbformat).collect(),
            execution_count,
            metadata,
            running: false,
        },
        v4::Cell::Markdown {
            id,
            metadata,
            source,
            attachments: _,
        } => {
            let text = source.concat();
            CellState {
                id,
                kind: CellKind::Markdown {
                    rendered: markdown::Content::parse(&text),
                    editing: false,
                },
                source: text_editor::Content::with_text(&text),
                outputs: Vec::new(),
                execution_count: None,
                metadata,
                running: false,
            }
        }
        v4::Cell::Raw {
            id,
            metadata,
            source,
        } => CellState {
            id,
            kind: CellKind::Raw,
            source: text_editor::Content::with_text(&source.concat()),
            outputs: Vec::new(),
            execution_count: None,
            metadata,
            running: false,
        },
    }
}

fn cell_to_nbformat(cell: &CellState) -> v4::Cell {
    let source = split_lines(&cell.source_text());
    match &cell.kind {
        CellKind::Code => v4::Cell::Code {
            id: cell.id.clone(),
            metadata: cell.metadata.clone(),
            execution_count: cell.execution_count,
            source,
            outputs: cell.outputs.iter().map(output_to_nbformat).collect(),
        },
        CellKind::Markdown { .. } => v4::Cell::Markdown {
            id: cell.id.clone(),
            metadata: cell.metadata.clone(),
            source,
            attachments: None,
        },
        CellKind::Raw => v4::Cell::Raw {
            id: cell.id.clone(),
            metadata: cell.metadata.clone(),
            source,
        },
    }
}

fn output_from_nbformat(output: v4::Output) -> CellOutput {
    match output {
        v4::Output::Stream { name, text } => CellOutput::Stream { name, text: text.0 },
        v4::Output::DisplayData(d) => CellOutput::Data {
            rendered: render::prepare(&d.data),
            media: d.data,
            execution_count: None,
        },
        v4::Output::ExecuteResult(r) => CellOutput::Data {
            rendered: render::prepare(&r.data),
            media: r.data,
            execution_count: Some(r.execution_count),
        },
        v4::Output::Error(e) => CellOutput::Error {
            ename: e.ename,
            evalue: e.evalue,
            traceback: e.traceback,
        },
    }
}

fn output_to_nbformat(output: &CellOutput) -> v4::Output {
    match output {
        CellOutput::Stream { name, text } => v4::Output::Stream {
            name: name.clone(),
            text: v4::MultilineString(text.clone()),
        },
        CellOutput::Data {
            media,
            execution_count: Some(count),
            ..
        } => v4::Output::ExecuteResult(v4::ExecuteResult {
            execution_count: *count,
            data: media.clone(),
            metadata: serde_json::Map::new(),
        }),
        CellOutput::Data {
            media,
            execution_count: None,
            ..
        } => v4::Output::DisplayData(v4::DisplayData {
            data: media.clone(),
            metadata: serde_json::Map::new(),
        }),
        CellOutput::Error {
            ename,
            evalue,
            traceback,
        } => v4::Output::Error(v4::ErrorOutput {
            ename: ename.clone(),
            evalue: evalue.clone(),
            traceback: traceback.clone(),
        }),
    }
}

/// nbformat convention: sources are stored as lines, each retaining its `\n`.
fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split_inclusive('\n').map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_round_trips_without_data_loss() {
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/smoke.ipynb"
        ));
        let doc = load(path).expect("fixture should load");
        assert!(doc.cells.len() >= 5);
        assert!(matches!(doc.cells[0].kind, CellKind::Markdown { .. }));

        let json = save(&doc).expect("serialize");
        let reparsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let original: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(reparsed["cells"].as_array().unwrap().len(),
                   original["cells"].as_array().unwrap().len());
        // Source text survives the line-split round trip exactly.
        for (a, b) in reparsed["cells"]
            .as_array()
            .unwrap()
            .iter()
            .zip(original["cells"].as_array().unwrap())
        {
            let join = |v: &serde_json::Value| -> String {
                v["source"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s.as_str().unwrap())
                    .collect()
            };
            assert_eq!(join(a), join(b));
        }
    }
}

pub fn new_code_cell() -> CellState {
    CellState {
        id: v4::CellId::from(uuid::Uuid::new_v4()),
        kind: CellKind::Code,
        source: text_editor::Content::new(),
        outputs: Vec::new(),
        execution_count: None,
        metadata: v4::CellMetadata::default(),
        running: false,
    }
}
