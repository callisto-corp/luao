use luao_lexer::{Span, TokenKind};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
    pub kind: ParseErrorKind,
}

#[derive(Debug, Clone)]
pub enum ParseErrorKind {
    UnexpectedToken { expected: String, found: TokenKind },
    UnexpectedEof,
    InvalidExpression,
    InvalidStatement,
    InvalidClassMember,
    InvalidEnumVariant,
    InvalidTypeAnnotation,
    DuplicateModifier(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}:{}] {}",
            self.span.start.line, self.span.start.column, self.message
        )
    }
}

impl std::error::Error for ParseError {}

pub type ParseResult<T> = Result<T, ParseError>;
