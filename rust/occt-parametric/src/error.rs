//! Model errors and kernel failure diagnostics located in the model.

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelError {
    pub message: String,
    /// Structured causes of a kernel failure, located in the model; empty
    /// for failures the kernel did not explain.
    pub diagnostics: Vec<FeatureDiagnostic>,
}

/// What OCCT reported about a failed feature, located by the feature's own
/// selectors and inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureDiagnostic {
    /// The feature whose operation failed.
    pub feature: String,
    pub kind: DiagnosticKind,
    /// OCCT's enumeration value for the kind.
    pub code: i32,
    /// OCCT's name for the code, such as `ChFiDS_WalkingFailure`.
    pub name: String,
    /// Index among the edges or faces the feature's selectors resolved to.
    pub selection: Option<usize>,
    /// Index of the feature's edge or face selector that produced the
    /// selection at fault.
    pub selector: Option<usize>,
    /// The boolean input, by output name, that the diagnostic concerns.
    pub input: Option<String>,
}

impl FeatureDiagnostic {
    pub(crate) fn from_kernel(diagnostic: Diagnostic) -> Self {
        Self {
            feature: String::new(),
            kind: diagnostic.kind,
            code: diagnostic.code,
            name: diagnostic.name,
            selection: diagnostic.input_index,
            selector: None,
            input: None,
        }
    }
}

impl ModelError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            diagnostics: Vec::new(),
        }
    }

    /// Prefixes the message, keeping the diagnostics.
    pub(crate) fn context(mut self, prefix: &str) -> Self {
        self.message = format!("{prefix}: {}", self.message);
        self
    }

    /// Attributes kernel diagnostics to selectors from the number of shapes
    /// each selector resolved to, and names them in the message.
    pub(crate) fn locate_selections(mut self, selector_sizes: &[usize], noun: &str) -> Self {
        let mut faulty = Vec::new();
        for diagnostic in &mut self.diagnostics {
            let Some(selection) = diagnostic.selection else {
                continue;
            };
            let mut end = 0;
            diagnostic.selector = selector_sizes.iter().position(|size| {
                end += size;
                selection < end
            });
            if let Some(selector) = diagnostic.selector
                && !faulty.contains(&selector)
            {
                faulty.push(selector);
            }
        }
        if !faulty.is_empty() {
            let list = faulty
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let plural = if faulty.len() == 1 { "" } else { "s" };
            self.message = format!("{}; at fault: {noun} selector{plural} {list}", self.message);
        }
        self
    }

    /// Attributes boolean diagnostics to the named inputs.
    pub(crate) fn locate_operands(mut self, operands: [&str; 2]) -> Self {
        let mut faulty = Vec::new();
        for diagnostic in &mut self.diagnostics {
            if let Some(operand) = diagnostic
                .selection
                .take()
                .and_then(|index| operands.get(index))
            {
                diagnostic.input = Some((*operand).to_owned());
                if !faulty.contains(operand) {
                    faulty.push(*operand);
                }
            }
        }
        if !faulty.is_empty() {
            let list = faulty
                .iter()
                .map(|name| format!("'{name}'"))
                .collect::<Vec<_>>()
                .join(", ");
            self.message = format!("{}; at fault: input {list}", self.message);
        }
        self
    }

    pub(crate) fn in_feature(mut self, feature: &str) -> Self {
        for diagnostic in &mut self.diagnostics {
            diagnostic.feature = feature.to_owned();
        }
        self.context(&format!("feature '{feature}'"))
    }
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ModelError {}

impl From<BridgeError> for ModelError {
    fn from(error: BridgeError) -> Self {
        Self {
            message: error.to_string(),
            diagnostics: error
                .diagnostics
                .into_iter()
                .map(FeatureDiagnostic::from_kernel)
                .collect(),
        }
    }
}
