use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DslState {
    /// Diagnostics produced by the most recent validation pass.
    pub diagnostics: Vec<crate::dsl::Diagnostic>,
}
