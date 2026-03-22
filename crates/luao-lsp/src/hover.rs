use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use crate::document::DocumentState;

pub fn hover_info(doc: &DocumentState, position: Position) -> Option<Hover> {
    let word = word_at_position(&doc.content, position)?;

    if let Some(desc) = keyword_description(&word) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**{}** (keyword)\n\n{}", word, desc),
            }),
            range: None,
        });
    }

    if let Some(ref sym_table) = doc.symbol_table {
        if let Some(class) = sym_table.lookup_class(&word) {
            let mut info = format!("**class {}**", class.name);
            if let Some(ref parent) = class.parent {
                info.push_str(&format!(" extends {}", parent));
            }
            if !class.interfaces.is_empty() {
                info.push_str(&format!(" implements {}", class.interfaces.join(", ")));
            }
            if class.is_abstract {
                info = format!("**abstract {}**", &info[2..]);
            }
            if class.is_sealed {
                info = format!("**sealed {}**", &info[2..]);
            }
            info.push_str(&format!("\n\nFields: {}", class.fields.len()));
            info.push_str(&format!("\nMethods: {}", class.methods.len()));
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: info,
                }),
                range: None,
            });
        }

        if let Some(iface) = sym_table.lookup_interface(&word) {
            let mut info = format!("**interface {}**", iface.name);
            if !iface.extends.is_empty() {
                info.push_str(&format!(" extends {}", iface.extends.join(", ")));
            }
            info.push_str(&format!("\n\nMethods: {}", iface.methods.len()));
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: info,
                }),
                range: None,
            });
        }

        if let Some(en) = sym_table.lookup_enum(&word) {
            let variants: Vec<_> = en.variants.iter().map(|v| v.name.as_str()).collect();
            let info = format!(
                "**enum {}**\n\nVariants: {}",
                en.name,
                variants.join(", ")
            );
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: info,
                }),
                range: None,
            });
        }

        for scope in &sym_table.scopes {
            if let Some((_, ty)) = scope.symbols.get(&word) {
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("**{}**: `{:?}`", word, ty),
                    }),
                    range: None,
                });
            }
        }
    }

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

fn keyword_description(word: &str) -> Option<&'static str> {
    match word {
        "class" => Some("Declares a new class."),
        "extends" => Some("Specifies the parent class in a class declaration."),
        "implements" => Some("Specifies interfaces that a class implements."),
        "interface" => Some("Declares a new interface."),
        "abstract" => Some("Marks a class or method as abstract."),
        "static" => Some("Marks a field or method as static (class-level)."),
        "public" => Some("Sets member visibility to public."),
        "private" => Some("Sets member visibility to private."),
        "protected" => Some("Sets member visibility to protected."),
        "readonly" => Some("Marks a field as read-only after initialization."),
        "super" => Some("References the parent class."),
        "new" => Some("Creates a new instance of a class."),
        "enum" => Some("Declares a new enumeration."),
        "sealed" => Some("Prevents a class from being extended."),
        "override" => Some("Marks a method as overriding a parent method."),
        "instanceof" => Some("Checks if an object is an instance of a class."),
        "local" => Some("Declares a local variable."),
        "function" => Some("Declares a function."),
        "if" => Some("Begins a conditional statement."),
        "then" => Some("Begins the body of an if/elseif clause."),
        "else" => Some("Begins the else branch of a conditional."),
        "elseif" => Some("Begins an additional conditional branch."),
        "end" => Some("Ends a block (function, if, for, while, class, etc.)."),
        "for" => Some("Begins a for loop."),
        "while" => Some("Begins a while loop."),
        "repeat" => Some("Begins a repeat-until loop."),
        "until" => Some("Specifies the condition for a repeat loop."),
        "do" => Some("Begins a do block or for-loop body."),
        "return" => Some("Returns values from a function."),
        "break" => Some("Exits the innermost loop."),
        "and" => Some("Logical AND operator."),
        "or" => Some("Logical OR operator."),
        "not" => Some("Logical NOT operator."),
        "nil" => Some("The nil value, representing absence of a value."),
        "true" => Some("Boolean true literal."),
        "false" => Some("Boolean false literal."),
        "in" => Some("Used in generic for loops."),
        "goto" => Some("Jumps to a label."),
        "get" => Some("Defines a property getter."),
        "set" => Some("Defines a property setter."),
        "switch" => Some("Begins a switch statement. Syntax: switch expr do case val then ... end"),
        "case" => Some("Defines a case branch in a switch statement. Supports multiple comma-separated values."),
        "default" => Some("Defines the default branch in a switch statement."),
        _ => None,
    }
}
