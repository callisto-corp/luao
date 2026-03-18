use luao_lexer::{Lexer, TokenKind};
use tower_lsp::lsp_types::SemanticToken;

use crate::document::DocumentState;

pub const TOKEN_TYPES: &[&str] = &[
    "keyword",
    "class",
    "type",
    "property",
    "enum",
    "enumMember",
    "function",
    "method",
    "variable",
    "parameter",
    "string",
    "number",
    "operator",
];

pub const TOKEN_TYPE_KEYWORD: u32 = 0;
pub const TOKEN_TYPE_CLASS: u32 = 1;
pub const TOKEN_TYPE_TYPE: u32 = 2;
pub const TOKEN_TYPE_PROPERTY: u32 = 3;
pub const TOKEN_TYPE_ENUM: u32 = 4;
pub const TOKEN_TYPE_ENUM_MEMBER: u32 = 5;
pub const TOKEN_TYPE_FUNCTION: u32 = 6;
pub const TOKEN_TYPE_METHOD: u32 = 7;
pub const TOKEN_TYPE_VARIABLE: u32 = 8;
pub const TOKEN_TYPE_PARAMETER: u32 = 9;
pub const TOKEN_TYPE_STRING: u32 = 10;
pub const TOKEN_TYPE_NUMBER: u32 = 11;
pub const TOKEN_TYPE_OPERATOR: u32 = 12;

pub fn semantic_tokens(doc: &DocumentState) -> Vec<SemanticToken> {
    let tokens = Lexer::new(&doc.content).tokenize();
    let mut result = Vec::new();
    let mut prev_line: u32 = 0;
    let mut prev_char: u32 = 0;

    for token in &tokens {
        if token.kind == TokenKind::Eof || token.kind == TokenKind::Error {
            continue;
        }

        let token_type = match token.kind {
            k if k.is_keyword() => match k {
                TokenKind::Class | TokenKind::Interface | TokenKind::Enum => TOKEN_TYPE_TYPE,
                _ => TOKEN_TYPE_KEYWORD,
            },
            TokenKind::Identifier => classify_identifier(doc, token),
            TokenKind::Number => TOKEN_TYPE_NUMBER,
            TokenKind::StringLiteral => TOKEN_TYPE_STRING,
            _ => TOKEN_TYPE_OPERATOR,
        };

        let line = token.span.start.line.saturating_sub(1);
        let char_pos = token.span.start.column.saturating_sub(1);
        let length = token.lexeme.len() as u32;

        let delta_line = line - prev_line;
        let delta_start = if delta_line == 0 {
            char_pos - prev_char
        } else {
            char_pos
        };

        result.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: 0,
        });

        prev_line = line;
        prev_char = char_pos;
    }

    result
}

fn classify_identifier(doc: &DocumentState, token: &luao_lexer::Token) -> u32 {
    let name = token.lexeme.as_str();

    if let Some(ref sym_table) = doc.symbol_table {
        if sym_table.lookup_class(name).is_some() {
            return TOKEN_TYPE_CLASS;
        }
        if sym_table.lookup_interface(name).is_some() {
            return TOKEN_TYPE_TYPE;
        }
        if sym_table.lookup_enum(name).is_some() {
            return TOKEN_TYPE_ENUM;
        }
        for en in sym_table.enums.values() {
            for variant in &en.variants {
                if variant.name == name {
                    return TOKEN_TYPE_ENUM_MEMBER;
                }
            }
        }
    }

    TOKEN_TYPE_VARIABLE
}
