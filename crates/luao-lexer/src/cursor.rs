use crate::span::Position;

pub struct Cursor<'a> {
    source: &'a str,
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    offset: usize,
    line: u32,
    column: u32,
}

impl<'a> Cursor<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            chars: source.char_indices().peekable(),
            offset: 0,
            line: 1,
            column: 1,
        }
    }

    pub fn position(&self) -> Position {
        Position::new(self.offset, self.line, self.column)
    }

    pub fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|&(_, c)| c)
    }

    pub fn peek_next(&self) -> Option<char> {
        let mut iter = self.source[self.offset..].chars();
        iter.next();
        iter.next()
    }

    pub fn advance(&mut self) -> Option<char> {
        if let Some((idx, ch)) = self.chars.next() {
            self.offset = idx + ch.len_utf8();
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            Some(ch)
        } else {
            None
        }
    }

    pub fn is_at_end(&mut self) -> bool {
        self.chars.peek().is_none()
    }

    pub fn slice(&self, start: usize, end: usize) -> &'a str {
        &self.source[start..end]
    }

    pub fn remaining(&self) -> &'a str {
        &self.source[self.offset..]
    }

    pub fn offset(&self) -> usize {
        self.offset
    }
}
