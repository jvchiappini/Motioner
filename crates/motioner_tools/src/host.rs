//! The host a tool runs against.
//!
//! `motioner_app` implements [`Host`] with the same `Editor`, `Project` and
//! resolver stack the window uses, so a mutating tool gets write-through,
//! checkpoints and undo for free. Read-only hosts implement only the document
//! getters; the default methods report [`ToolError::RequiresHarness`].

use std::path::Path;

use motioner_analysis::{GrayFrame, RgbaImage};
use motioner_core::{Document, TextMetrics};
use motioner_dsl::ValueOrigins;
use motioner_render::{FontSet, ImageData, ImageSet};
use serde_json::Value;

use crate::error::ToolError;

/// Everything a tool may read or ask the host to do.
pub trait Host {
    /// The open program path.
    fn program_path(&self) -> &Path;

    /// The program source.
    fn program_source(&self) -> &str;

    /// The compiled document.
    fn document(&self) -> &Document;

    /// Value origins of the last run, when the host kept them.
    fn origins(&self) -> Option<&ValueOrigins>;

    /// Decoded images the renderer may draw.
    fn images(&self) -> &ImageSet;

    /// The font set the renderer uses.
    fn fonts(&self) -> &FontSet;

    /// Text metrics matching the renderer (D-0035).
    fn text_metrics(&self) -> &dyn TextMetrics;

    /// `project_summary` content, already shaped as the tool result.
    fn summary(&self) -> Value;

    /// Decoded frames of every visible video node at `frame`.
    ///
    /// # Errors
    ///
    /// Hosts that cannot decode video return an empty list.
    fn video_images(&mut self, _frame: i64) -> Result<Vec<ImageData>, ToolError> {
        Ok(Vec::new())
    }

    /// One imported image decoded, with its registry key.
    ///
    /// # Errors
    ///
    /// Returns the host's decoding error.
    fn asset_image(&mut self, _key: &str) -> Result<Option<(String, RgbaImage)>, ToolError> {
        Ok(None)
    }

    /// One audio asset as mono samples and their rate.
    ///
    /// # Errors
    ///
    /// Returns the host's decoding error.
    fn asset_audio(&mut self, _key: &str) -> Result<Option<(u32, Vec<f32>)>, ToolError> {
        Ok(None)
    }

    /// Low-resolution greyscale frames of a video asset.
    ///
    /// # Errors
    ///
    /// Returns the host's decoding error.
    fn asset_video(&mut self, _key: &str, _max: u32) -> Result<Option<Vec<GrayFrame>>, ToolError> {
        Ok(None)
    }

    /// Runs D-0024 command text through the editor.
    ///
    /// # Errors
    ///
    /// Returns the editor's typed error.
    fn run_commands(&mut self, _commands: &[String]) -> Result<Value, ToolError> {
        Err(ToolError::RequiresHarness(
            "run_commands needs an open editor (prompt 14 harness or `motioner tool`)".to_owned(),
        ))
    }

    /// Applies code edits through the write-through planner.
    ///
    /// # Errors
    ///
    /// Returns the planner's typed error.
    fn apply_code_edit(&mut self, _file: &str, _edits: &[Value]) -> Result<Value, ToolError> {
        Err(ToolError::RequiresHarness(
            "apply_code_edit needs an open editor".to_owned(),
        ))
    }

    /// Applies one rig edit through the editor's write-through sink.
    ///
    /// # Errors
    ///
    /// Returns the planner's refusal reason or a compile error.
    fn rig_edit(&mut self, _edit: &motioner_dsl::RigEdit) -> Result<Value, ToolError> {
        Err(ToolError::RequiresHarness(
            "rig edits need an open editor (prompt 14 harness or `motioner tool`)".to_owned(),
        ))
    }

    /// Writes a new program file.
    ///
    /// # Errors
    ///
    /// Returns the diagnostic or I/O error.
    fn write_file(&mut self, _file: &str, _contents: &str) -> Result<Value, ToolError> {
        Err(ToolError::RequiresHarness(
            "write_file needs an open project".to_owned(),
        ))
    }

    /// Recompiles the program.
    ///
    /// # Errors
    ///
    /// Returns the first diagnostic.
    fn run_program(&mut self) -> Result<Value, ToolError> {
        Err(ToolError::RequiresHarness(
            "run_program needs an open editor".to_owned(),
        ))
    }

    /// Formats a program.
    ///
    /// # Errors
    ///
    /// Returns the formatter error.
    fn format(&mut self, _file: Option<&str>) -> Result<Value, ToolError> {
        Err(ToolError::RequiresHarness(
            "format needs an open editor".to_owned(),
        ))
    }

    /// Lints a program.
    ///
    /// # Errors
    ///
    /// Returns the linter error.
    fn lint(&mut self, _file: Option<&str>) -> Result<Value, ToolError> {
        Err(ToolError::RequiresHarness(
            "lint needs an open editor".to_owned(),
        ))
    }

    /// Undoes edits.
    ///
    /// # Errors
    ///
    /// Returns the editor's error.
    fn undo(&mut self, _steps: u32) -> Result<Value, ToolError> {
        Err(ToolError::RequiresHarness(
            "undo needs an open editor".to_owned(),
        ))
    }

    /// Redoes edits.
    ///
    /// # Errors
    ///
    /// Returns the editor's error.
    fn redo(&mut self, _steps: u32) -> Result<Value, ToolError> {
        Err(ToolError::RequiresHarness(
            "redo needs an open editor".to_owned(),
        ))
    }

    /// Names the current program state.
    ///
    /// # Errors
    ///
    /// Returns the editor's error.
    fn checkpoint(&mut self, _name: &str) -> Result<String, ToolError> {
        Err(ToolError::RequiresHarness(
            "checkpoint needs an open editor".to_owned(),
        ))
    }

    /// Restores a named checkpoint.
    ///
    /// # Errors
    ///
    /// Returns the editor's error.
    fn restore(&mut self, _checkpoint: &str) -> Result<Value, ToolError> {
        Err(ToolError::RequiresHarness(
            "restore needs an open editor".to_owned(),
        ))
    }

    /// Renders a low-quality preview MP4.
    ///
    /// # Errors
    ///
    /// Returns the encoder's error.
    fn render_preview(&mut self, _start: i64, _end: i64) -> Result<Value, ToolError> {
        Err(ToolError::RequiresHarness(
            "render_preview needs the application encoder".to_owned(),
        ))
    }

    /// Exports the final MP4.
    ///
    /// # Errors
    ///
    /// Returns the encoder's error.
    fn export(&mut self, _path: Option<&str>) -> Result<Value, ToolError> {
        Err(ToolError::RequiresHarness(
            "export needs the application encoder".to_owned(),
        ))
    }

    /// Reads or writes the brand kit.
    ///
    /// # Errors
    ///
    /// Returns the project error.
    fn brand_kit(&mut self, _action: &str, _values: Option<&Value>) -> Result<Value, ToolError> {
        Err(ToolError::RequiresHarness(
            "brand_kit needs an open project".to_owned(),
        ))
    }
}
