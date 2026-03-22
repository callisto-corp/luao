use crate::span::Span;
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: SmolStr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    And,
    Break,
    Continue,
    Do,
    Else,
    ElseIf,
    End,
    False,
    For,
    Function,
    If,
    In,
    Local,
    Nil,
    Not,
    Or,
    Repeat,
    Return,
    Then,
    True,
    Until,
    While,

    Class,
    Extends,
    Implements,
    Interface,
    Abstract,
    Static,
    Public,
    Private,
    Protected,
    Readonly,
    Super,
    New,
    Enum,
    Sealed,
    Get,
    Set,
    Override,
    Instanceof,
    Extern,
    As,
    Type,
    Import,
    Export,
    From,
    Async,
    Await,
    Yield,
    Generator,
    Switch,
    Case,
    Default,
    Void,

    Identifier,
    Number,
    StringLiteral,

    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,

    Dot,
    DotDot,
    DotDotDot,
    Colon,
    Semicolon,
    Comma,
    QuestionMark,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Hash,
    Ampersand,
    Tilde,
    Pipe,
    ShiftLeft,
    ShiftRight,
    DoubleSlash,

    Equal,
    NotEqual,
    LessThan,
    LessEqual,
    GreaterThan,
    GreaterEqual,
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    CaretAssign,
    DotDotAssign,

    LeftAngle,
    RightAngle,

    Arrow,

    Eof,
    Error,
}

impl TokenKind {
    pub fn from_keyword(s: &str) -> Option<TokenKind> {
        match s {
            "and" => Some(TokenKind::And),
            "break" => Some(TokenKind::Break),
            "continue" => Some(TokenKind::Continue),
            "do" => Some(TokenKind::Do),
            "else" => Some(TokenKind::Else),
            "elseif" => Some(TokenKind::ElseIf),
            "end" => Some(TokenKind::End),
            "false" => Some(TokenKind::False),
            "for" => Some(TokenKind::For),
            "function" => Some(TokenKind::Function),
            "if" => Some(TokenKind::If),
            "in" => Some(TokenKind::In),
            "local" => Some(TokenKind::Local),
            "nil" => Some(TokenKind::Nil),
            "not" => Some(TokenKind::Not),
            "or" => Some(TokenKind::Or),
            "repeat" => Some(TokenKind::Repeat),
            "return" => Some(TokenKind::Return),
            "then" => Some(TokenKind::Then),
            "true" => Some(TokenKind::True),
            "until" => Some(TokenKind::Until),
            "while" => Some(TokenKind::While),
            "class" => Some(TokenKind::Class),
            "extends" => Some(TokenKind::Extends),
            "implements" => Some(TokenKind::Implements),
            "interface" => Some(TokenKind::Interface),
            "abstract" => Some(TokenKind::Abstract),
            "static" => Some(TokenKind::Static),
            "public" => Some(TokenKind::Public),
            "private" => Some(TokenKind::Private),
            "protected" => Some(TokenKind::Protected),
            "readonly" => Some(TokenKind::Readonly),
            "super" => Some(TokenKind::Super),
            "new" => Some(TokenKind::New),
            "enum" => Some(TokenKind::Enum),
            "sealed" => Some(TokenKind::Sealed),
            "get" => Some(TokenKind::Get),
            "set" => Some(TokenKind::Set),
            "override" => Some(TokenKind::Override),
            "instanceof" => Some(TokenKind::Instanceof),
            "extern" => Some(TokenKind::Extern),
            "as" => Some(TokenKind::As),
            "type" => Some(TokenKind::Type),
            "import" => Some(TokenKind::Import),
            "export" => Some(TokenKind::Export),
            "from" => Some(TokenKind::From),
            "async" => Some(TokenKind::Async),
            "await" => Some(TokenKind::Await),
            "yield" => Some(TokenKind::Yield),
            "generator" => Some(TokenKind::Generator),
            "switch" => Some(TokenKind::Switch),
            "case" => Some(TokenKind::Case),
            "default" => Some(TokenKind::Default),
            "void" => Some(TokenKind::Void),
            _ => None,
        }
    }

    /// Contextual keywords — reserved only in Luao syntactic positions,
    /// usable as identifiers in plain Lua code (e.g. `type(f)`).
    pub fn is_contextual_keyword(&self) -> bool {
        matches!(
            self,
            TokenKind::Class
                | TokenKind::Extends
                | TokenKind::Implements
                | TokenKind::Interface
                | TokenKind::Abstract
                | TokenKind::Static
                | TokenKind::Public
                | TokenKind::Private
                | TokenKind::Protected
                | TokenKind::Readonly
                | TokenKind::Super
                | TokenKind::New
                | TokenKind::Enum
                | TokenKind::Sealed
                | TokenKind::Get
                | TokenKind::Set
                | TokenKind::Override
                | TokenKind::Instanceof
                | TokenKind::Extern
                | TokenKind::As
                | TokenKind::Type
                | TokenKind::Import
                | TokenKind::Export
                | TokenKind::From
                | TokenKind::Async
                | TokenKind::Await
                | TokenKind::Yield
                | TokenKind::Generator
                | TokenKind::Switch
                | TokenKind::Case
                | TokenKind::Default
                | TokenKind::Void
        )
    }

    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            TokenKind::And
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::Do
                | TokenKind::Else
                | TokenKind::ElseIf
                | TokenKind::End
                | TokenKind::False
                | TokenKind::For
                | TokenKind::Function
                | TokenKind::If
                | TokenKind::In
                | TokenKind::Local
                | TokenKind::Nil
                | TokenKind::Not
                | TokenKind::Or
                | TokenKind::Repeat
                | TokenKind::Return
                | TokenKind::Then
                | TokenKind::True
                | TokenKind::Until
                | TokenKind::While
                | TokenKind::Class
                | TokenKind::Extends
                | TokenKind::Implements
                | TokenKind::Interface
                | TokenKind::Abstract
                | TokenKind::Static
                | TokenKind::Public
                | TokenKind::Private
                | TokenKind::Protected
                | TokenKind::Readonly
                | TokenKind::Super
                | TokenKind::New
                | TokenKind::Enum
                | TokenKind::Sealed
                | TokenKind::Get
                | TokenKind::Set
                | TokenKind::Override
                | TokenKind::Instanceof
                | TokenKind::Extern
                | TokenKind::As
                | TokenKind::Type
                | TokenKind::Import
                | TokenKind::Export
                | TokenKind::From
                | TokenKind::Async
                | TokenKind::Await
                | TokenKind::Yield
                | TokenKind::Generator
                | TokenKind::Switch
                | TokenKind::Case
                | TokenKind::Default
                | TokenKind::Void
        )
    }
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::And => write!(f, "and"),
            TokenKind::Break => write!(f, "break"),
            TokenKind::Continue => write!(f, "continue"),
            TokenKind::Do => write!(f, "do"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::ElseIf => write!(f, "elseif"),
            TokenKind::End => write!(f, "end"),
            TokenKind::False => write!(f, "false"),
            TokenKind::For => write!(f, "for"),
            TokenKind::Function => write!(f, "function"),
            TokenKind::If => write!(f, "if"),
            TokenKind::In => write!(f, "in"),
            TokenKind::Local => write!(f, "local"),
            TokenKind::Nil => write!(f, "nil"),
            TokenKind::Not => write!(f, "not"),
            TokenKind::Or => write!(f, "or"),
            TokenKind::Repeat => write!(f, "repeat"),
            TokenKind::Return => write!(f, "return"),
            TokenKind::Then => write!(f, "then"),
            TokenKind::True => write!(f, "true"),
            TokenKind::Until => write!(f, "until"),
            TokenKind::While => write!(f, "while"),
            TokenKind::Class => write!(f, "class"),
            TokenKind::Extends => write!(f, "extends"),
            TokenKind::Implements => write!(f, "implements"),
            TokenKind::Interface => write!(f, "interface"),
            TokenKind::Abstract => write!(f, "abstract"),
            TokenKind::Static => write!(f, "static"),
            TokenKind::Public => write!(f, "public"),
            TokenKind::Private => write!(f, "private"),
            TokenKind::Protected => write!(f, "protected"),
            TokenKind::Readonly => write!(f, "readonly"),
            TokenKind::Super => write!(f, "super"),
            TokenKind::New => write!(f, "new"),
            TokenKind::Enum => write!(f, "enum"),
            TokenKind::Sealed => write!(f, "sealed"),
            TokenKind::Get => write!(f, "get"),
            TokenKind::Set => write!(f, "set"),
            TokenKind::Override => write!(f, "override"),
            TokenKind::Instanceof => write!(f, "instanceof"),
            TokenKind::Extern => write!(f, "extern"),
            TokenKind::As => write!(f, "as"),
            TokenKind::Type => write!(f, "type"),
            TokenKind::Import => write!(f, "import"),
            TokenKind::Export => write!(f, "export"),
            TokenKind::From => write!(f, "from"),
            TokenKind::Async => write!(f, "async"),
            TokenKind::Await => write!(f, "await"),
            TokenKind::Yield => write!(f, "yield"),
            TokenKind::Generator => write!(f, "generator"),
            TokenKind::Switch => write!(f, "switch"),
            TokenKind::Case => write!(f, "case"),
            TokenKind::Default => write!(f, "default"),
            TokenKind::Void => write!(f, "void"),
            TokenKind::Identifier => write!(f, "identifier"),
            TokenKind::Number => write!(f, "number"),
            TokenKind::StringLiteral => write!(f, "string"),
            TokenKind::LeftParen => write!(f, "("),
            TokenKind::RightParen => write!(f, ")"),
            TokenKind::LeftBracket => write!(f, "["),
            TokenKind::RightBracket => write!(f, "]"),
            TokenKind::LeftBrace => write!(f, "{{"),
            TokenKind::RightBrace => write!(f, "}}"),
            TokenKind::Dot => write!(f, "."),
            TokenKind::DotDot => write!(f, ".."),
            TokenKind::DotDotDot => write!(f, "..."),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Semicolon => write!(f, ";"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::QuestionMark => write!(f, "?"),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::Caret => write!(f, "^"),
            TokenKind::Hash => write!(f, "#"),
            TokenKind::Ampersand => write!(f, "&"),
            TokenKind::Tilde => write!(f, "~"),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::ShiftLeft => write!(f, "<<"),
            TokenKind::ShiftRight => write!(f, ">>"),
            TokenKind::DoubleSlash => write!(f, "//"),
            TokenKind::Equal => write!(f, "=="),
            TokenKind::NotEqual => write!(f, "~="),
            TokenKind::LessThan => write!(f, "<"),
            TokenKind::LessEqual => write!(f, "<="),
            TokenKind::GreaterThan => write!(f, ">"),
            TokenKind::GreaterEqual => write!(f, ">="),
            TokenKind::Assign => write!(f, "="),
            TokenKind::PlusAssign => write!(f, "+="),
            TokenKind::MinusAssign => write!(f, "-="),
            TokenKind::StarAssign => write!(f, "*="),
            TokenKind::SlashAssign => write!(f, "/="),
            TokenKind::PercentAssign => write!(f, "%="),
            TokenKind::CaretAssign => write!(f, "^="),
            TokenKind::DotDotAssign => write!(f, "..="),
            TokenKind::LeftAngle => write!(f, "<"),
            TokenKind::RightAngle => write!(f, ">"),
            TokenKind::Arrow => write!(f, "->"),
            TokenKind::Eof => write!(f, "EOF"),
            TokenKind::Error => write!(f, "error"),
        }
    }
}
