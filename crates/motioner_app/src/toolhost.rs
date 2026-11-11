//! The [`motioner_tools::Host`] implementation used by `motioner tool`.
//!
//! It wraps the same `Editor` the window and `motioner edit` use, so a
//! tool-driven mutation goes through write-through, undo and the trace just
//! like a gesture. Nothing here bypasses the document API.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use motioner_analysis::{GrayFrame, RgbaImage};
use motioner_core::ImageResolver;
use motioner_core::{Document, TextMetrics};
use motioner_dsl::ValueOrigins;
use motioner_io::{decode_frames, decode_to_cache, FfmpegLocation, ProjectImages};
use motioner_render::{CpuTextMetrics, FontSet, ImageData, ImageSet};
use motioner_tools::{Host, ToolError};
use motioner_ui::{Command, Editor};
use serde_json::{json, Value};

use crate::{ensure_parent, image_set, video_images, InputTarget};

/// A host backed by an open editor.
pub struct AppHost {
    target: InputTarget,
    editor: Editor,
    document: Document,
    source: String,
    origins: Option<ValueOrigins>,
    images: ImageSet,
    project_images: ProjectImages,
    fonts: FontSet,
    metrics: Arc<CpuTextMetrics>,
    checkpoints: HashMap<String, String>,
}

impl AppHost {
    /// Opens `input` through the editor and snapshots the document.
    ///
    /// # Errors
    ///
    /// Returns the editor's typed error when the program does not compile.
    pub fn open(input: &Path) -> Result<Self, ToolError> {
        let target = crate::resolve_input(input).map_err(|failure| match failure {
            crate::Failure::Diagnostic(message) => ToolError::Host(message),
            crate::Failure::Error(error) => ToolError::Host(format!("{error:#}")),
        })?;
        let fonts = crate::project_fonts(&target);
        let metrics = Arc::new(CpuTextMetrics::new(fonts.clone()));
        let mut editor = Editor::open_with_metrics(&target.composition, metrics.clone())
            .map_err(|error| ToolError::Host(error.to_string()))?;
        let project_images = match &target.project {
            Some(project) => ProjectImages::new(project),
            None => ProjectImages::loose(&target.root),
        };
        editor
            .session_mut()
            .set_image_resolver(std::rc::Rc::new(match &target.project {
                Some(project) => ProjectImages::new(project),
                None => ProjectImages::loose(&target.root),
            }));
        let _ = editor.session_mut().reload();
        let document = editor
            .session()
            .document()
            .cloned()
            .ok_or_else(|| ToolError::Host("the program did not compile".to_owned()))?;
        let source = editor.session().source().to_owned();
        let origins = editor.session().origins().cloned();
        let images = image_set(&project_images);
        Ok(Self {
            target,
            editor,
            document,
            source,
            origins,
            images,
            project_images,
            fonts,
            metrics,
            checkpoints: HashMap::new(),
        })
    }

    /// Re-snapshots the document after a mutation.
    fn refresh(&mut self) {
        if let Some(document) = self.editor.session().document() {
            self.document = document.clone();
        }
        self.source = self.editor.session().source().to_owned();
        self.origins = self.editor.session().origins().cloned();
    }

    fn replace_source(&mut self, source: &str) -> Result<(), ToolError> {
        self.editor
            .session_mut()
            .edit_program(source, Instant::now());
        self.editor
            .flush_now()
            .map_err(|error| ToolError::Host(error.to_string()))?;
        self.refresh();
        Ok(())
    }

    fn asset_path(&self, key: &str) -> Option<PathBuf> {
        let project = self.target.project.as_ref()?;
        let entry = project.find_asset(key)?;
        Some(project.assets_dir().join(entry.file_name()))
    }
}

impl Host for AppHost {
    fn program_path(&self) -> &Path {
        &self.target.composition
    }

    fn program_source(&self) -> &str {
        &self.source
    }

    fn document(&self) -> &Document {
        &self.document
    }

    fn origins(&self) -> Option<&ValueOrigins> {
        self.origins.as_ref()
    }

    fn images(&self) -> &ImageSet {
        &self.images
    }

    fn fonts(&self) -> &FontSet {
        &self.fonts
    }

    fn text_metrics(&self) -> &dyn TextMetrics {
        self.metrics.as_ref()
    }

    fn summary(&self) -> Value {
        let composition = self.document.composition();
        let contracts = self.document.len();
        json!({
            "comps": [composition.name()],
            "width": composition.width(),
            "height": composition.height(),
            "fps": format!(
                "{}/{}",
                composition.frame_rate().numerator(),
                composition.frame_rate().denominator()
            ),
            "duration": composition.duration(),
            "layers": contracts,
            "assets": self.target.project.as_ref().map_or(0, |project| project.assets().len()),
            "overrides": self.editor.session().overrides().len(),
            "std_version": motioner_dsl::std_version(),
        })
    }

    fn video_images(&mut self, frame: i64) -> Result<Vec<ImageData>, ToolError> {
        video_images(&self.project_images, &self.document, frame)
            .map_err(|failure| ToolError::Host(format!("{failure}")))
    }

    fn asset_image(&mut self, key: &str) -> Result<Option<(String, RgbaImage)>, ToolError> {
        let Some(project) = self.target.project.as_ref() else {
            return Ok(None);
        };
        let Some(entry) = project.find_asset(key) else {
            return Ok(None);
        };
        let images = self.project_images.decoded();
        for image in images {
            if image.key.to_hex() == entry.hash {
                return Ok(Some((
                    entry.original.clone(),
                    RgbaImage {
                        width: image.width,
                        height: image.height,
                        rgba8: image.rgba8,
                    },
                )));
            }
        }
        // Not in the registry cache: decode the file now.
        let resolver = ProjectImages::new(project);
        if let Ok(asset) = resolver.resolve(&entry.original) {
            if let Some(image) = resolver
                .decoded()
                .into_iter()
                .find(|image| image.key == asset.key)
            {
                return Ok(Some((
                    entry.original.clone(),
                    RgbaImage {
                        width: image.width,
                        height: image.height,
                        rgba8: image.rgba8,
                    },
                )));
            }
        }
        Ok(None)
    }

    fn asset_audio(&mut self, key: &str) -> Result<Option<(u32, Vec<f32>)>, ToolError> {
        let Some(project) = self.target.project.as_ref() else {
            return Ok(None);
        };
        let Some(entry) = project.find_asset(key) else {
            return Ok(None);
        };
        let pcm = decode_to_cache(
            &project.assets_dir().join(entry.file_name()),
            &project.cache_dir(),
            &entry.hash,
            &FfmpegLocation::FromEnvOrPath,
        )
        .map_err(|error| ToolError::Host(error.to_string()))?;
        // Stereo interleaved to mono.
        let mono: Vec<f32> = pcm
            .samples
            .chunks_exact(2)
            .map(|pair| (pair[0] + pair[1]) * 0.5)
            .collect();
        Ok(Some((pcm.sample_rate, mono)))
    }

    fn asset_video(&mut self, key: &str, max: u32) -> Result<Option<Vec<GrayFrame>>, ToolError> {
        let Some(path) = self.asset_path(key) else {
            return Ok(None);
        };
        let count = max.clamp(4, 240) as usize;
        let frames = decode_frames(&path, 0.0, 96, 54, count, &FfmpegLocation::FromEnvOrPath)
            .map_err(|error| ToolError::Host(error.to_string()))?;
        let gray = frames
            .into_iter()
            .map(|frame| {
                let luma = frame
                    .chunks_exact(4)
                    .map(|pixel| {
                        let r = f64::from(pixel[0]);
                        let g = f64::from(pixel[1]);
                        let b = f64::from(pixel[2]);
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let value = (0.2126 * r + 0.7152 * g + 0.0722 * b).round() as u8;
                        value
                    })
                    .collect();
                GrayFrame {
                    width: 96,
                    height: 54,
                    luma,
                }
            })
            .collect();
        Ok(Some(gray))
    }

    fn run_commands(&mut self, commands: &[String]) -> Result<Value, ToolError> {
        let mut executed = 0_usize;
        for text in commands {
            let parsed = motioner_ui::parse_commands(text)
                .map_err(|error| ToolError::Host(error.to_string()))?;
            for command in parsed {
                self.editor
                    .execute(&command, Duration::ZERO)
                    .map_err(|error| ToolError::Host(error.to_string()))?;
                executed += 1;
            }
        }
        self.editor
            .flush_now()
            .map_err(|error| ToolError::Host(error.to_string()))?;
        self.refresh();
        let (undo, _) = self.editor.history();
        Ok(json!({
            "ok": true,
            "commands": executed,
            "diff": self.editor.last_diff(),
            "checkpoint": format!("undo:{undo}"),
        }))
    }

    fn apply_code_edit(&mut self, file: &str, edits: &[Value]) -> Result<Value, ToolError> {
        let path = self.target.composition.clone();
        let same_file = Path::new(file)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                path.file_name()
                    .and_then(|open| open.to_str())
                    .is_some_and(|open| open == name)
            });
        if !file.is_empty() && !same_file {
            return Err(ToolError::InvalidArgs(format!(
                "only the open program can be edited with apply_code_edit (got {file:?})"
            )));
        }
        let mut text = self.source.clone();
        for edit in edits {
            let object = edit
                .as_object()
                .ok_or_else(|| ToolError::InvalidArgs("edits must be objects".to_owned()))?;
            if let Some(old) = object.get("old_text").and_then(Value::as_str) {
                let new = object
                    .get("new_text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::InvalidArgs("new_text is required".to_owned()))?;
                if !text.contains(old) {
                    return Err(ToolError::InvalidArgs(format!(
                        "old_text not found: {old:?}"
                    )));
                }
                text = text.replacen(old, new, 1);
                continue;
            }
            let start = object
                .get("start_line")
                .and_then(Value::as_i64)
                .ok_or_else(|| {
                    ToolError::InvalidArgs("an edit needs start_line or old_text".to_owned())
                })?;
            let end = object
                .get("end_line")
                .and_then(Value::as_i64)
                .unwrap_or(start);
            let new = object
                .get("new_text")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::InvalidArgs("new_text is required".to_owned()))?;
            let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
            let start = usize::try_from(start.max(1) - 1)
                .unwrap_or(0)
                .min(lines.len());
            let end = usize::try_from(end.max(1))
                .unwrap_or(start)
                .min(lines.len());
            lines.splice(start..end, new.lines().map(str::to_owned));
            text = lines.join("\n");
            if !text.ends_with('\n') {
                text.push('\n');
            }
        }
        let diff = motioner_ui::unified_diff(&self.source, &text, "program");
        self.replace_source(&text)?;
        let (undo, _) = self.editor.history();
        Ok(json!({
            "ok": true,
            "diff": diff,
            "checkpoint": format!("undo:{undo}"),
            "diagnostics": self.diagnostics_json(),
        }))
    }

    fn write_file(&mut self, file: &str, contents: &str) -> Result<Value, ToolError> {
        let root = self
            .target
            .composition
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let path = root.join(file);
        if !path.starts_with(&root) {
            return Err(ToolError::InvalidArgs(
                "the file must stay inside the project".to_owned(),
            ));
        }
        if path.exists() {
            return Err(ToolError::InvalidArgs(format!(
                "{} already exists; edit it instead of overwriting",
                path.display()
            )));
        }
        ensure_parent(&path).map_err(|failure| ToolError::Host(format!("{failure}")))?;
        std::fs::write(&path, contents).map_err(|error| ToolError::Host(error.to_string()))?;
        Ok(json!({"ok": true, "path": path.display().to_string()}))
    }

    fn run_program(&mut self) -> Result<Value, ToolError> {
        self.editor
            .session_mut()
            .reload()
            .map_err(|error| ToolError::Host(error.to_string()))?;
        self.refresh();
        Ok(json!({
            "ok": self.editor.session().document().is_some(),
            "diagnostics": self.diagnostics_json(),
        }))
    }

    fn format(&mut self, _file: Option<&str>) -> Result<Value, ToolError> {
        let formatted = motioner_dsl::printer::format(&self.source)
            .map_err(|error| ToolError::Host(error.to_string()))?;
        let diff = motioner_ui::unified_diff(&self.source, &formatted, "program");
        if formatted != self.source {
            self.replace_source(&formatted)?;
        }
        Ok(json!({"ok": true, "diff": diff}))
    }

    fn lint(&mut self, _file: Option<&str>) -> Result<Value, ToolError> {
        let diagnostics = motioner_dsl::lint::lint(&self.source)
            .map_err(|error| ToolError::Host(error.to_string()))?;
        let list: Vec<Value> = diagnostics
            .iter()
            .map(|diagnostic| {
                json!({
                    "severity": format!("{:?}", diagnostic.severity).to_lowercase(),
                    "category": diagnostic.category.name(),
                    "line": diagnostic.span.line,
                    "message": diagnostic.message,
                })
            })
            .collect();
        Ok(json!({"ok": true, "diagnostics": list}))
    }

    fn undo(&mut self, steps: u32) -> Result<Value, ToolError> {
        for _ in 0..steps {
            self.editor
                .execute(&Command::Undo, Duration::ZERO)
                .map_err(|error| ToolError::Host(error.to_string()))?;
        }
        self.editor
            .flush_now()
            .map_err(|error| ToolError::Host(error.to_string()))?;
        self.refresh();
        let (undo, redo) = self.editor.history();
        Ok(json!({"ok": true, "undo": undo, "redo": redo, "diff": self.editor.last_diff()}))
    }

    fn redo(&mut self, steps: u32) -> Result<Value, ToolError> {
        for _ in 0..steps {
            self.editor
                .execute(&Command::Redo, Duration::ZERO)
                .map_err(|error| ToolError::Host(error.to_string()))?;
        }
        self.editor
            .flush_now()
            .map_err(|error| ToolError::Host(error.to_string()))?;
        self.refresh();
        let (undo, redo) = self.editor.history();
        Ok(json!({"ok": true, "undo": undo, "redo": redo, "diff": self.editor.last_diff()}))
    }

    fn checkpoint(&mut self, name: &str) -> Result<String, ToolError> {
        self.checkpoints
            .insert(name.to_owned(), self.source.clone());
        Ok(name.to_owned())
    }

    fn restore(&mut self, checkpoint: &str) -> Result<Value, ToolError> {
        let Some(source) = self.checkpoints.get(checkpoint).cloned() else {
            return Err(ToolError::InvalidArgs(format!(
                "no checkpoint {checkpoint:?}"
            )));
        };
        let diff = motioner_ui::unified_diff(&self.source, &source, "program");
        self.replace_source(&source)?;
        Ok(json!({"ok": true, "diff": diff}))
    }

    fn brand_kit(&mut self, action: &str, values: Option<&Value>) -> Result<Value, ToolError> {
        let project = self
            .target
            .project
            .as_ref()
            .ok_or_else(|| ToolError::Host("brand_kit needs a project".to_owned()))?;
        let path = project.root().join("brand.json");
        match action {
            "get" => {
                let text = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_owned());
                let value = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
                Ok(value)
            }
            "set" => {
                let value = values.cloned().unwrap_or_else(|| json!({}));
                std::fs::write(
                    &path,
                    serde_json::to_string_pretty(&value).unwrap_or_default(),
                )
                .map_err(|error| ToolError::Host(error.to_string()))?;
                Ok(value)
            }
            other => Err(ToolError::InvalidArgs(format!(
                "brand_kit action must be get or set, got {other:?}"
            ))),
        }
    }
}

impl AppHost {
    fn diagnostics_json(&self) -> Vec<Value> {
        self.editor
            .session()
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                json!({
                    "severity": if diagnostic.warning { "warning" } else { "error" },
                    "line": diagnostic.line,
                    "column": diagnostic.column,
                    "message": diagnostic.message,
                })
            })
            .collect()
    }
}
