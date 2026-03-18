use tower_lsp::lsp_types;

pub fn to_lsp_diagnostics(diagnostics: &[luao_checker::Diagnostic]) -> Vec<lsp_types::Diagnostic> {
    diagnostics.iter().map(convert_diagnostic).collect()
}

fn convert_diagnostic(diag: &luao_checker::Diagnostic) -> lsp_types::Diagnostic {
    let range = span_to_range(diag.span);
    let severity = match diag.severity {
        luao_checker::DiagnosticSeverity::Error => Some(lsp_types::DiagnosticSeverity::ERROR),
        luao_checker::DiagnosticSeverity::Warning => Some(lsp_types::DiagnosticSeverity::WARNING),
        luao_checker::DiagnosticSeverity::Info => Some(lsp_types::DiagnosticSeverity::INFORMATION),
    };

    lsp_types::Diagnostic {
        range,
        severity,
        code: Some(lsp_types::NumberOrString::String(diag.code.clone())),
        source: Some("luao".to_string()),
        message: diag.message.clone(),
        ..Default::default()
    }
}

pub fn span_to_range(span: luao_lexer::Span) -> lsp_types::Range {
    lsp_types::Range {
        start: lsp_types::Position {
            line: span.start.line.saturating_sub(1),
            character: span.start.column.saturating_sub(1),
        },
        end: lsp_types::Position {
            line: span.end.line.saturating_sub(1),
            character: span.end.column.saturating_sub(1),
        },
    }
}
