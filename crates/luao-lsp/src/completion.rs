use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Position};

use crate::document::DocumentState;

const LUAO_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while", "class",
    "extends", "implements", "interface", "abstract", "static", "public", "private", "protected",
    "readonly", "super", "new", "enum", "sealed", "get", "set", "override", "instanceof",
];

pub async fn complete(doc: &DocumentState, position: Position) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    let trigger = find_trigger_context(&doc.content, position);

    match trigger {
        TriggerContext::DotAccess(name) => {
            if let Some(ref sym_table) = doc.symbol_table {
                if let Some(class) = sym_table.lookup_class(&name) {
                    for field in &class.fields {
                        items.push(CompletionItem {
                            label: field.name.clone(),
                            kind: Some(CompletionItemKind::FIELD),
                            detail: Some(format!("{:?}", field.type_info)),
                            ..Default::default()
                        });
                    }
                    for method in &class.methods {
                        items.push(CompletionItem {
                            label: method.name.clone(),
                            kind: Some(CompletionItemKind::METHOD),
                            detail: Some(format!("{:?}", method.return_type)),
                            ..Default::default()
                        });
                    }
                }
            }
        }
        TriggerContext::ColonAccess(name) => {
            if let Some(ref sym_table) = doc.symbol_table {
                if let Some(class) = sym_table.lookup_class(&name) {
                    for method in &class.methods {
                        if !method.is_static {
                            items.push(CompletionItem {
                                label: method.name.clone(),
                                kind: Some(CompletionItemKind::METHOD),
                                detail: Some(format!("{:?}", method.return_type)),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
        TriggerContext::TypeAnnotation => {
            if let Some(ref sym_table) = doc.symbol_table {
                for name in sym_table.classes.keys() {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::CLASS),
                        ..Default::default()
                    });
                }
                for name in sym_table.interfaces.keys() {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::INTERFACE),
                        ..Default::default()
                    });
                }
                for name in sym_table.enums.keys() {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::ENUM),
                        ..Default::default()
                    });
                }
                for builtin in &["number", "string", "boolean", "nil", "any", "void"] {
                    items.push(CompletionItem {
                        label: builtin.to_string(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        ..Default::default()
                    });
                }
            }
        }
        TriggerContext::General => {
            for kw in LUAO_KEYWORDS {
                items.push(CompletionItem {
                    label: kw.to_string(),
                    kind: Some(CompletionItemKind::KEYWORD),
                    ..Default::default()
                });
            }
            if let Some(ref sym_table) = doc.symbol_table {
                for name in sym_table.classes.keys() {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::CLASS),
                        ..Default::default()
                    });
                }
                for name in sym_table.interfaces.keys() {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::INTERFACE),
                        ..Default::default()
                    });
                }
                for name in sym_table.enums.keys() {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::ENUM),
                        ..Default::default()
                    });
                }
            }
        }
    }

    items
}

enum TriggerContext {
    DotAccess(String),
    ColonAccess(String),
    TypeAnnotation,
    General,
}

fn find_trigger_context(content: &str, position: Position) -> TriggerContext {
    let line_idx = position.line as usize;
    let col_idx = position.character as usize;

    let line = match content.lines().nth(line_idx) {
        Some(l) => l,
        None => return TriggerContext::General,
    };

    let before_cursor = if col_idx <= line.len() {
        &line[..col_idx]
    } else {
        line
    };

    let trimmed = before_cursor.trim_end();

    if trimmed.ends_with('.') {
        let word = extract_word_before(trimmed, trimmed.len() - 1);
        if !word.is_empty() {
            return TriggerContext::DotAccess(word);
        }
    }

    if trimmed.ends_with(':') {
        let before_colon = &trimmed[..trimmed.len() - 1];
        if before_colon.ends_with(|c: char| c.is_alphanumeric() || c == '_') {
            let word = extract_word_before(trimmed, trimmed.len() - 1);
            if !word.is_empty() {
                if before_colon.contains("local ") || before_colon.contains("function ") {
                    return TriggerContext::TypeAnnotation;
                }
                return TriggerContext::ColonAccess(word);
            }
        }
        return TriggerContext::TypeAnnotation;
    }

    TriggerContext::General
}

fn extract_word_before(s: &str, pos: usize) -> String {
    let bytes = s.as_bytes();
    let mut end = pos;
    while end > 0 && (bytes[end - 1].is_ascii_alphanumeric() || bytes[end - 1] == b'_') {
        end -= 1;
    }
    s[end..pos].to_string()
}
