//! `motioner tool`: run one agent tool headless and print its JSON result.

use std::path::PathBuf;

use anyhow::anyhow;
use motioner_tools::{execute, Runtime, ToolError};
use serde_json::Value;

use crate::toolhost::AppHost;
use crate::Failure;

/// Arguments of `tool`.
#[derive(Debug, PartialEq, Eq)]
pub struct ToolArgs {
    /// Project, manifest or composition.
    pub input: PathBuf,
    /// Tool name (omitted with `--batch`).
    pub name: Option<String>,
    /// `--args` JSON object.
    pub args: Option<String>,
    /// Where images are written; `out/tools` by default.
    pub out: Option<PathBuf>,
    /// JSONL batch file.
    pub batch: Option<PathBuf>,
}

fn error_json(error: &ToolError) -> String {
    serde_json::json!({
        "ok": false,
        "error": {
            "kind": error.kind(),
            "message": error.message(),
        }
    })
    .to_string()
}

fn out_dir(args: &ToolArgs) -> PathBuf {
    args.out
        .clone()
        .unwrap_or_else(|| PathBuf::from("out/tools"))
}

fn run_one(
    host: &mut AppHost,
    runtime: &mut Runtime,
    name: &str,
    args: &Value,
    out: &std::path::Path,
) -> Result<Value, ToolError> {
    let output = execute(name, args, runtime, host, out)?;
    Ok(output.result)
}

/// Runs the tool command; `Ok(false)` means the call failed after printing a
/// JSON error.
pub fn run(args: &ToolArgs) -> Result<bool, Failure> {
    let out = out_dir(args);
    let mut host =
        AppHost::open(&args.input).map_err(|error| Failure::Error(anyhow!(error.to_string())))?;
    let mut runtime = Runtime::default();

    if let Some(batch) = &args.batch {
        let text = std::fs::read_to_string(batch)
            .map_err(|error| Failure::Error(anyhow!("cannot read {}: {error}", batch.display())))?;
        let mut results = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let entry: Value = serde_json::from_str(line)
                .map_err(|error| Failure::Error(anyhow!("batch line {}: {error}", index + 1)))?;
            let name = entry
                .get("tool")
                .or_else(|| entry.get("name"))
                .and_then(Value::as_str)
                .ok_or_else(|| Failure::Error(anyhow!("batch line {} has no `tool`", index + 1)))?;
            let call_args = entry
                .get("args")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::default()));
            match run_one(&mut host, &mut runtime, name, &call_args, &out) {
                Ok(result) => results.push(serde_json::json!({
                    "tool": name,
                    "ok": true,
                    "result": result,
                })),
                Err(error) => {
                    println!("{}", error_json(&error));
                    return Ok(false);
                }
            }
        }
        println!("{}", serde_json::json!({"ok": true, "results": results}));
        return Ok(true);
    }

    let name = args
        .name
        .as_deref()
        .ok_or_else(|| Failure::Error(anyhow!("tool needs a name")))?;
    let call_args: Value = match &args.args {
        Some(text) => serde_json::from_str(text)
            .map_err(|error| Failure::Error(anyhow!("--args is not valid JSON: {error}")))?,
        None => Value::Object(serde_json::Map::default()),
    };
    match run_one(&mut host, &mut runtime, name, &call_args, &out) {
        Ok(result) => {
            println!("{}", serde_json::json!({"ok": true, "result": result}));
            Ok(true)
        }
        Err(error) => {
            println!("{}", error_json(&error));
            Ok(false)
        }
    }
}
