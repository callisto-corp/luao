use tower_lsp::lsp_types::{DocumentSymbol, SymbolKind};

use crate::diagnostics::span_to_range;
use crate::document::DocumentState;

#[allow(deprecated)]
pub fn document_symbols(doc: &DocumentState) -> Vec<DocumentSymbol> {
    let ast = match doc.ast.as_ref() {
        Some(ast) => ast,
        None => return Vec::new(),
    };

    let mut symbols = Vec::new();

    for stmt in &ast.statements {
        match stmt {
            luao_parser::Statement::ClassDecl(c) => {
                let mut children = Vec::new();

                for member in &c.members {
                    match member {
                        luao_parser::ClassMember::Field(f) => {
                            children.push(DocumentSymbol {
                                name: f.name.name.to_string(),
                                detail: f.type_annotation.as_ref().map(|_| "field".to_string()),
                                kind: SymbolKind::FIELD,
                                tags: None,
                                deprecated: None,
                                range: span_to_range(f.span),
                                selection_range: span_to_range(f.name.span),
                                children: None,
                            });
                        }
                        luao_parser::ClassMember::Method(m) => {
                            children.push(DocumentSymbol {
                                name: m.name.name.to_string(),
                                detail: Some("method".to_string()),
                                kind: SymbolKind::METHOD,
                                tags: None,
                                deprecated: None,
                                range: span_to_range(m.span),
                                selection_range: span_to_range(m.name.span),
                                children: None,
                            });
                        }
                        luao_parser::ClassMember::Constructor(con) => {
                            children.push(DocumentSymbol {
                                name: "constructor".to_string(),
                                detail: Some("constructor".to_string()),
                                kind: SymbolKind::CONSTRUCTOR,
                                tags: None,
                                deprecated: None,
                                range: span_to_range(con.span),
                                selection_range: span_to_range(con.span),
                                children: None,
                            });
                        }
                        luao_parser::ClassMember::Property(p) => {
                            children.push(DocumentSymbol {
                                name: p.name.name.to_string(),
                                detail: Some("property".to_string()),
                                kind: SymbolKind::PROPERTY,
                                tags: None,
                                deprecated: None,
                                range: span_to_range(p.span),
                                selection_range: span_to_range(p.name.span),
                                children: None,
                            });
                        }
                    }
                }

                symbols.push(DocumentSymbol {
                    name: c.name.name.to_string(),
                    detail: Some("class".to_string()),
                    kind: SymbolKind::CLASS,
                    tags: None,
                    deprecated: None,
                    range: span_to_range(c.span),
                    selection_range: span_to_range(c.name.span),
                    children: if children.is_empty() {
                        None
                    } else {
                        Some(children)
                    },
                });
            }
            luao_parser::Statement::InterfaceDecl(i) => {
                let children: Vec<DocumentSymbol> = i
                    .methods
                    .iter()
                    .map(|m| DocumentSymbol {
                        name: m.name.name.to_string(),
                        detail: Some("method".to_string()),
                        kind: SymbolKind::METHOD,
                        tags: None,
                        deprecated: None,
                        range: span_to_range(m.span),
                        selection_range: span_to_range(m.name.span),
                        children: None,
                    })
                    .collect();

                symbols.push(DocumentSymbol {
                    name: i.name.name.to_string(),
                    detail: Some("interface".to_string()),
                    kind: SymbolKind::INTERFACE,
                    tags: None,
                    deprecated: None,
                    range: span_to_range(i.span),
                    selection_range: span_to_range(i.name.span),
                    children: if children.is_empty() {
                        None
                    } else {
                        Some(children)
                    },
                });
            }
            luao_parser::Statement::EnumDecl(e) => {
                let children: Vec<DocumentSymbol> = e
                    .variants
                    .iter()
                    .map(|v| DocumentSymbol {
                        name: v.name.name.to_string(),
                        detail: Some("variant".to_string()),
                        kind: SymbolKind::ENUM_MEMBER,
                        tags: None,
                        deprecated: None,
                        range: span_to_range(v.span),
                        selection_range: span_to_range(v.name.span),
                        children: None,
                    })
                    .collect();

                symbols.push(DocumentSymbol {
                    name: e.name.name.to_string(),
                    detail: Some("enum".to_string()),
                    kind: SymbolKind::ENUM,
                    tags: None,
                    deprecated: None,
                    range: span_to_range(e.span),
                    selection_range: span_to_range(e.name.span),
                    children: if children.is_empty() {
                        None
                    } else {
                        Some(children)
                    },
                });
            }
            luao_parser::Statement::FunctionDecl(f) => {
                let name = f
                    .name
                    .parts
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                symbols.push(DocumentSymbol {
                    name,
                    detail: Some("function".to_string()),
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    range: span_to_range(f.span),
                    selection_range: span_to_range(f.name.span),
                    children: None,
                });
            }
            _ => {}
        }
    }

    symbols
}
