use crate::cursor::Cursor;
use crate::span::{Position, Span};
use crate::token::{Token, TokenKind};
use smol_str::SmolStr;

pub struct Lexer<'a> {
    cursor: Cursor<'a>,
    _source: &'a str,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            cursor: Cursor::new(source),
            _source: source,
        }
    }

    pub fn tokenize(mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        tokens
    }

    fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();

        if self.cursor.is_at_end() {
            return self.make_token(TokenKind::Eof, self.cursor.position(), self.cursor.position());
        }

        let start = self.cursor.position();

        match self.cursor.peek() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => self.lex_identifier_or_keyword(start),
            Some(c) if c.is_ascii_digit() => self.lex_number(start),
            Some('"') | Some('\'') => self.lex_string(start),
            Some('[') => {
                let next = self.cursor.peek_next();
                if next == Some('[') || next == Some('=') {
                    if self.is_long_string_start() {
                        self.lex_long_string(start)
                    } else {
                        self.lex_symbol(start)
                    }
                } else {
                    self.lex_symbol(start)
                }
            }
            Some(_) => self.lex_symbol(start),
            None => self.make_token(TokenKind::Eof, start, start),
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.cursor.peek() {
                Some(c) if c.is_ascii_whitespace() => {
                    self.cursor.advance();
                }
                Some('-') => {
                    if self.cursor.peek_next() == Some('-') {
                        self.skip_comment();
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
    }

    fn skip_comment(&mut self) {
        self.cursor.advance();
        self.cursor.advance();

        if self.cursor.peek() == Some('[') {
            let remaining = self.cursor.remaining();
            if let Some(level) = self.long_bracket_level(remaining) {
                self.skip_long_comment(level);
                return;
            }
        }

        while let Some(c) = self.cursor.peek() {
            if c == '\n' {
                break;
            }
            self.cursor.advance();
        }
    }

    fn skip_long_comment(&mut self, level: usize) {
        self.cursor.advance();
        for _ in 0..level {
            self.cursor.advance();
        }
        self.cursor.advance();

        let closing = format!("]{}]", "=".repeat(level));
        loop {
            if self.cursor.is_at_end() {
                break;
            }
            if self.cursor.remaining().starts_with(&closing) {
                for _ in 0..closing.len() {
                    self.cursor.advance();
                }
                break;
            }
            self.cursor.advance();
        }
    }

    fn long_bracket_level(&self, s: &str) -> Option<usize> {
        if !s.starts_with('[') {
            return None;
        }
        let mut level = 0;
        let mut chars = s.chars().skip(1);
        loop {
            match chars.next() {
                Some('=') => level += 1,
                Some('[') => return Some(level),
                _ => return None,
            }
        }
    }

    fn is_long_string_start(&self) -> bool {
        self.long_bracket_level(self.cursor.remaining()).is_some()
    }

    fn lex_identifier_or_keyword(&mut self, start: Position) -> Token {
        let start_offset = self.cursor.offset();

        while let Some(c) = self.cursor.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.cursor.advance();
            } else {
                break;
            }
        }

        let end = self.cursor.position();
        let lexeme = self.cursor.slice(start_offset, self.cursor.offset());
        let kind = TokenKind::from_keyword(lexeme).unwrap_or(TokenKind::Identifier);

        Token {
            kind,
            lexeme: SmolStr::new(lexeme),
            span: Span::new(start, end),
        }
    }

    fn lex_number(&mut self, start: Position) -> Token {
        let start_offset = self.cursor.offset();

        if self.cursor.peek() == Some('0')
            && (self.cursor.peek_next() == Some('x') || self.cursor.peek_next() == Some('X'))
        {
            self.cursor.advance();
            self.cursor.advance();
            while let Some(c) = self.cursor.peek() {
                if c.is_ascii_hexdigit() || c == '_' {
                    self.cursor.advance();
                } else {
                    break;
                }
            }
        } else {
            while let Some(c) = self.cursor.peek() {
                if c.is_ascii_digit() || c == '_' {
                    self.cursor.advance();
                } else {
                    break;
                }
            }

            if self.cursor.peek() == Some('.') {
                if let Some(next) = self.cursor.peek_next() {
                    if next.is_ascii_digit() {
                        self.cursor.advance();
                        while let Some(c) = self.cursor.peek() {
                            if c.is_ascii_digit() || c == '_' {
                                self.cursor.advance();
                            } else {
                                break;
                            }
                        }
                    }
                }
            }

            if let Some('e' | 'E') = self.cursor.peek() {
                self.cursor.advance();
                if let Some('+' | '-') = self.cursor.peek() {
                    self.cursor.advance();
                }
                while let Some(c) = self.cursor.peek() {
                    if c.is_ascii_digit() {
                        self.cursor.advance();
                    } else {
                        break;
                    }
                }
            }
        }

        let end = self.cursor.position();
        let lexeme = self.cursor.slice(start_offset, self.cursor.offset());

        Token {
            kind: TokenKind::Number,
            lexeme: SmolStr::new(lexeme),
            span: Span::new(start, end),
        }
    }

    fn lex_string(&mut self, start: Position) -> Token {
        let start_offset = self.cursor.offset();
        let quote = self.cursor.advance().unwrap();

        loop {
            match self.cursor.peek() {
                None => break,
                Some('\\') => {
                    self.cursor.advance();
                    self.cursor.advance();
                }
                Some(c) if c == quote => {
                    self.cursor.advance();
                    break;
                }
                Some('\n') => break,
                _ => {
                    self.cursor.advance();
                }
            }
        }

        let end = self.cursor.position();
        let lexeme = self.cursor.slice(start_offset, self.cursor.offset());

        Token {
            kind: TokenKind::StringLiteral,
            lexeme: SmolStr::new(lexeme),
            span: Span::new(start, end),
        }
    }

    fn lex_long_string(&mut self, start: Position) -> Token {
        let start_offset = self.cursor.offset();
        let level = self.long_bracket_level(self.cursor.remaining()).unwrap();

        self.cursor.advance();
        for _ in 0..level {
            self.cursor.advance();
        }
        self.cursor.advance();

        let closing = format!("]{}]", "=".repeat(level));

        loop {
            if self.cursor.is_at_end() {
                break;
            }
            if self.cursor.remaining().starts_with(&closing) {
                for _ in 0..closing.len() {
                    self.cursor.advance();
                }
                break;
            }
            self.cursor.advance();
        }

        let end = self.cursor.position();
        let lexeme = self.cursor.slice(start_offset, self.cursor.offset());

        Token {
            kind: TokenKind::StringLiteral,
            lexeme: SmolStr::new(lexeme),
            span: Span::new(start, end),
        }
    }

    fn lex_symbol(&mut self, start: Position) -> Token {
        let ch = self.cursor.advance().unwrap();
        let start_offset = start.offset;

        let kind = match ch {
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            '[' => TokenKind::LeftBracket,
            ']' => TokenKind::RightBracket,
            '{' => TokenKind::LeftBrace,
            '}' => TokenKind::RightBrace,
            ';' => TokenKind::Semicolon,
            ',' => TokenKind::Comma,
            '+' => TokenKind::Plus,
            '*' => TokenKind::Star,
            '%' => TokenKind::Percent,
            '^' => TokenKind::Caret,
            '#' => TokenKind::Hash,
            '&' => TokenKind::Ampersand,
            '|' => TokenKind::Pipe,
            '.' => {
                if self.cursor.peek() == Some('.') {
                    self.cursor.advance();
                    if self.cursor.peek() == Some('.') {
                        self.cursor.advance();
                        TokenKind::DotDotDot
                    } else {
                        TokenKind::DotDot
                    }
                } else {
                    TokenKind::Dot
                }
            }
            ':' => {
                if self.cursor.peek() == Some(':') {
                    self.cursor.advance();
                    TokenKind::ColonColon
                } else {
                    TokenKind::Colon
                }
            }
            '-' => {
                if self.cursor.peek() == Some('>') {
                    self.cursor.advance();
                    TokenKind::Arrow
                } else {
                    TokenKind::Minus
                }
            }
            '/' => {
                if self.cursor.peek() == Some('/') {
                    self.cursor.advance();
                    TokenKind::DoubleSlash
                } else {
                    TokenKind::Slash
                }
            }
            '~' => {
                if self.cursor.peek() == Some('=') {
                    self.cursor.advance();
                    TokenKind::NotEqual
                } else {
                    TokenKind::Tilde
                }
            }
            '<' => {
                if self.cursor.peek() == Some('=') {
                    self.cursor.advance();
                    TokenKind::LessEqual
                } else if self.cursor.peek() == Some('<') {
                    self.cursor.advance();
                    TokenKind::ShiftLeft
                } else {
                    TokenKind::LessThan
                }
            }
            '>' => {
                if self.cursor.peek() == Some('=') {
                    self.cursor.advance();
                    TokenKind::GreaterEqual
                } else if self.cursor.peek() == Some('>') {
                    self.cursor.advance();
                    TokenKind::ShiftRight
                } else {
                    TokenKind::GreaterThan
                }
            }
            '=' => {
                if self.cursor.peek() == Some('=') {
                    self.cursor.advance();
                    TokenKind::Equal
                } else {
                    TokenKind::Assign
                }
            }
            _ => TokenKind::Error,
        };

        let end = self.cursor.position();
        let lexeme = self.cursor.slice(start_offset, self.cursor.offset());

        Token {
            kind,
            lexeme: SmolStr::new(lexeme),
            span: Span::new(start, end),
        }
    }

    fn make_token(&self, kind: TokenKind, start: Position, end: Position) -> Token {
        Token {
            kind,
            lexeme: SmolStr::new(""),
            span: Span::new(start, end),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(source: &str) -> Vec<Token> {
        Lexer::new(source).tokenize()
    }

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(source).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn test_empty() {
        assert_eq!(kinds(""), vec![TokenKind::Eof]);
    }

    #[test]
    fn test_keywords() {
        assert_eq!(
            kinds("class extends implements interface abstract"),
            vec![
                TokenKind::Class,
                TokenKind::Extends,
                TokenKind::Implements,
                TokenKind::Interface,
                TokenKind::Abstract,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_lua_keywords() {
        assert_eq!(
            kinds("if then else end"),
            vec![
                TokenKind::If,
                TokenKind::Then,
                TokenKind::Else,
                TokenKind::End,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_identifier() {
        let tokens = lex("myVar");
        assert_eq!(tokens[0].kind, TokenKind::Identifier);
        assert_eq!(tokens[0].lexeme.as_str(), "myVar");
    }

    #[test]
    fn test_number() {
        assert_eq!(
            kinds("42 3.14 0xFF 1e10"),
            vec![
                TokenKind::Number,
                TokenKind::Number,
                TokenKind::Number,
                TokenKind::Number,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_string() {
        assert_eq!(
            kinds(r#""hello" 'world'"#),
            vec![
                TokenKind::StringLiteral,
                TokenKind::StringLiteral,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_operators() {
        assert_eq!(
            kinds("+ - * / // % ^ == ~= <= >= < > = .. ..."),
            vec![
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::DoubleSlash,
                TokenKind::Percent,
                TokenKind::Caret,
                TokenKind::Equal,
                TokenKind::NotEqual,
                TokenKind::LessEqual,
                TokenKind::GreaterEqual,
                TokenKind::LessThan,
                TokenKind::GreaterThan,
                TokenKind::Assign,
                TokenKind::DotDot,
                TokenKind::DotDotDot,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_arrow() {
        assert_eq!(
            kinds("->"),
            vec![TokenKind::Arrow, TokenKind::Eof]
        );
    }

    #[test]
    fn test_comments_skipped() {
        assert_eq!(
            kinds("x -- this is a comment\ny"),
            vec![TokenKind::Identifier, TokenKind::Identifier, TokenKind::Eof]
        );
    }

    #[test]
    fn test_long_comment() {
        assert_eq!(
            kinds("x --[[ long comment ]] y"),
            vec![TokenKind::Identifier, TokenKind::Identifier, TokenKind::Eof]
        );
    }

    #[test]
    fn test_long_string() {
        let tokens = lex("[[hello world]]");
        assert_eq!(tokens[0].kind, TokenKind::StringLiteral);
        assert_eq!(tokens[0].lexeme.as_str(), "[[hello world]]");
    }

    #[test]
    fn test_class_declaration() {
        assert_eq!(
            kinds("class Dog extends Animal"),
            vec![
                TokenKind::Class,
                TokenKind::Identifier,
                TokenKind::Extends,
                TokenKind::Identifier,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_instanceof() {
        assert_eq!(
            kinds("x instanceof Foo"),
            vec![
                TokenKind::Identifier,
                TokenKind::Instanceof,
                TokenKind::Identifier,
                TokenKind::Eof,
            ]
        );
    }
}
