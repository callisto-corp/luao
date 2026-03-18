use tower_lsp::lsp_types::{Location, Position, Url};

use crate::document::DocumentState;

pub fn goto_definition(doc: &DocumentState, position: Position, uri: &Url) -> Option<Location> {
    let word = word_at_position(&doc.content, position)?;
    let sym_table = doc.symbol_table.as_ref()?;
    let ast = doc.ast.as_ref()?;

    for stmt in &ast.statements {
        match stmt {
            luao_parser::Statement::ClassDecl(c) if c.name.name.as_str() == word => {
                return Some(Location {
                    uri: uri.clone(),
                    range: crate::diagnostics::span_to_range(c.name.span),
                });
            }
            luao_parser::Statement::InterfaceDecl(i) if i.name.name.as_str() == word => {
                return Some(Location {
                    uri: uri.clone(),
                    range: crate::diagnostics::span_to_range(i.name.span),
                });
            }
            luao_parser::Statement::EnumDecl(e) if e.name.name.as_str() == word => {
                return Some(Location {
                    uri: uri.clone(),
                    range: crate::diagnostics::span_to_range(e.name.span),
                });
            }
            luao_parser::Statement::FunctionDecl(f) => {
                if let Some(first) = f.name.parts.first() {
                    if first.name.as_str() == word {
                        return Some(Location {
                            uri: uri.clone(),
                            range: crate::diagnostics::span_to_range(first.span),
                        });
                    }
                }
            }
            luao_parser::Statement::LocalAssignment(la) => {
                for name in &la.names {
                    if name.name.as_str() == word {
                        return Some(Location {
                            uri: uri.clone(),
                            range: crate::diagnostics::span_to_range(name.span),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    let _ = sym_table;
    None
}

fn word_at_position(content: &str, position: Position) -> Option<String> {
    let line = content.lines().nth(position.line as usize)?;
    let col = position.character as usize;
    if col > line.len() {
        return None;
    }

    let bytes = line.as_bytes();
    let mut start = col;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }

    let mut end = col;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }

    if start == end {
        return None;
    }

    Some(line[start..end].to_string())
}
