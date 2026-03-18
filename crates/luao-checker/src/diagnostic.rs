use std::fmt;

use luao_lexer::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
    pub severity: DiagnosticSeverity,
    pub code: String,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: Span, code: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span,
            severity: DiagnosticSeverity::Error,
            code: code.into(),
        }
    }

    pub fn warning(message: impl Into<String>, span: Span, code: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span,
            severity: DiagnosticSeverity::Warning,
            code: code.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Info => "info",
        };
        write!(
            f,
            "[{}] {} ({}:{}): {}",
            self.code, severity, self.span.start.line, self.span.start.column, self.message
        )
    }
}
