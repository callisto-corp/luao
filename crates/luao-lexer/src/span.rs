#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    pub offset: usize,
    pub line: u32,
    pub column: u32,
}

impl Span {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub fn merge(self, other: Span) -> Span {
        Span {
            start: if self.start.offset < other.start.offset {
                self.start
            } else {
                other.start
            },
            end: if self.end.offset > other.end.offset {
                self.end
            } else {
                other.end
            },
        }
    }

    pub fn empty() -> Self {
        Self {
            start: Position {
                offset: 0,
                line: 1,
                column: 1,
            },
            end: Position {
                offset: 0,
                line: 1,
                column: 1,
            },
        }
    }
}

impl Position {
    pub fn new(offset: usize, line: u32, column: u32) -> Self {
        Self {
            offset,
            line,
            column,
        }
    }
}
