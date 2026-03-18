//! Lua/Luau minifier — tokenizer-based, no external dependencies.
//!
//! Two-pass approach:
//! 1. Tokenize source into flat token stream.
//! 2. Walk tokens with a recursive descent mini-parser that tracks scopes,
//!    binds locals, renames identifiers, and emits minimal output.

use std::collections::HashMap;

// ── Token types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Keyword(String),
    Number(String),
    StringLit(String),
    Punct(String),
}

// ── Tokenizer ──────────────────────────────────────────────────────────

const LUA_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for",
    "function", "goto", "if", "in", "local", "nil", "not", "or",
    "repeat", "return", "then", "true", "until", "while", "continue",
];

fn tokenize(src: &str) -> Vec<Tok> {
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < len {
        let b = bytes[i];

        if b.is_ascii_whitespace() {
            i += 1;
            while i < len && bytes[i].is_ascii_whitespace() { i += 1; }
            continue;
        }

        // Long comment --[=*[
        if i + 2 < len && bytes[i] == b'-' && bytes[i+1] == b'-' && bytes[i+2] == b'[' {
            let mut j = i + 3;
            let mut eq = 0;
            while j < len && bytes[j] == b'=' { eq += 1; j += 1; }
            if j < len && bytes[j] == b'[' {
                j += 1;
                let close: Vec<u8> = {
                    let mut v = vec![b']'];
                    for _ in 0..eq { v.push(b'='); }
                    v.push(b']');
                    v
                };
                while j + close.len() <= len {
                    if bytes[j..j+close.len()] == close[..] { j += close.len(); break; }
                    j += 1;
                }
                i = j;
                continue;
            }
        }

        // Line comment
        if i + 1 < len && bytes[i] == b'-' && bytes[i+1] == b'-' {
            while i < len && bytes[i] != b'\n' { i += 1; }
            continue;
        }

        // Long string [=*[
        if bytes[i] == b'[' {
            let mut j = i + 1;
            let mut eq = 0;
            while j < len && bytes[j] == b'=' { eq += 1; j += 1; }
            if j < len && bytes[j] == b'[' {
                j += 1;
                let close: Vec<u8> = {
                    let mut v = vec![b']'];
                    for _ in 0..eq { v.push(b'='); }
                    v.push(b']');
                    v
                };
                let start = i;
                while j + close.len() <= len {
                    if bytes[j..j+close.len()] == close[..] { j += close.len(); break; }
                    j += 1;
                }
                tokens.push(Tok::StringLit(String::from_utf8_lossy(&bytes[start..j]).into()));
                i = j;
                continue;
            }
        }

        // Strings
        if b == b'"' || b == b'\'' {
            let q = b;
            let start = i;
            i += 1;
            while i < len && bytes[i] != q {
                if bytes[i] == b'\\' { i += 1; }
                i += 1;
            }
            if i < len { i += 1; }
            tokens.push(Tok::StringLit(String::from_utf8_lossy(&bytes[start..i]).into()));
            continue;
        }

        // Numbers
        if b.is_ascii_digit() || (b == b'.' && i+1 < len && bytes[i+1].is_ascii_digit()) {
            let start = i;
            if b == b'0' && i+1 < len && (bytes[i+1] == b'x' || bytes[i+1] == b'X') {
                i += 2;
                while i < len && bytes[i].is_ascii_hexdigit() { i += 1; }
            } else {
                while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'.') { i += 1; }
                if i < len && (bytes[i] == b'e' || bytes[i] == b'E') {
                    i += 1;
                    if i < len && (bytes[i] == b'+' || bytes[i] == b'-') { i += 1; }
                    while i < len && bytes[i].is_ascii_digit() { i += 1; }
                }
            }
            tokens.push(Tok::Number(String::from_utf8_lossy(&bytes[start..i]).into()));
            continue;
        }

        // Identifiers / keywords
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') { i += 1; }
            let w: String = String::from_utf8_lossy(&bytes[start..i]).into();
            if LUA_KEYWORDS.contains(&w.as_str()) {
                tokens.push(Tok::Keyword(w));
            } else {
                tokens.push(Tok::Ident(w));
            }
            continue;
        }

        // 3-char punct
        if i+2 < len {
            let s = String::from_utf8_lossy(&bytes[i..i+3]);
            if s == "..." || s == "..=" {
                tokens.push(Tok::Punct(s.into()));
                i += 3;
                continue;
            }
        }
        // 2-char punct
        if i+1 < len {
            let s = String::from_utf8_lossy(&bytes[i..i+2]);
            match s.as_ref() {
                "==" | "~=" | "<=" | ">=" | ".." | "//" | "<<" | ">>" | "->" |
                "+=" | "-=" | "*=" | "/=" | "%=" | "^=" => {
                    tokens.push(Tok::Punct(s.into()));
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }

        tokens.push(Tok::Punct(String::from(b as char)));
        i += 1;
    }
    tokens
}

// ── Name generation ────────────────────────────────────────────────────

const FIRST: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_";
const REST: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789";

fn idx_to_name(mut idx: usize) -> String {
    let fc = FIRST.len();
    let rc = REST.len();
    if idx < fc { return String::from(FIRST[idx] as char); }
    idx -= fc;
    let mut length: u32 = 2;
    let mut count = fc * rc;
    while idx >= count { idx -= count; length += 1; count *= rc; }
    let mut s = String::with_capacity(length as usize);
    let rp = rc.pow(length - 1);
    s.push(FIRST[idx / rp] as char);
    idx %= rp;
    for i in (0..length-1).rev() { let d = rc.pow(i); s.push(REST[idx / d] as char); idx %= d; }
    s
}

// ── Scope / Renamer ────────────────────────────────────────────────────

struct Renamer {
    scopes: Vec<HashMap<String, String>>,
    counter: usize,
    no_self: bool,
}

impl Renamer {
    fn new(no_self: bool) -> Self {
        Self { scopes: vec![HashMap::new()], counter: 0, no_self }
    }
    fn push(&mut self) { self.scopes.push(HashMap::new()); }
    fn pop(&mut self) { if self.scopes.len() > 1 { self.scopes.pop(); } }
    fn next_name(&mut self) -> String {
        loop {
            let n = idx_to_name(self.counter);
            self.counter += 1;
            if !LUA_KEYWORDS.contains(&n.as_str()) { return n; }
        }
    }
    fn bind(&mut self, name: &str) {
        if name == "..." { return; }
        if name == "self" && !self.no_self { return; }
        let short = self.next_name();
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), short);
        }
    }
    fn resolve(&self, name: &str) -> String {
        if name == "self" && !self.no_self { return name.to_string(); }
        for scope in self.scopes.iter().rev() {
            if let Some(short) = scope.get(name) { return short.clone(); }
        }
        name.to_string()
    }
}

// ── Emitter ────────────────────────────────────────────────────────────

struct Emitter {
    out: String,
    last_was_number: bool,
}

impl Emitter {
    fn new() -> Self { Self { out: String::new(), last_was_number: false } }

    fn emit(&mut self, text: &str) {
        self.emit_inner(text, false);
    }

    fn emit_number(&mut self, text: &str) {
        self.emit_inner(text, true);
    }

    fn emit_inner(&mut self, text: &str, is_number: bool) {
        if text.is_empty() { return; }
        if let (Some(&prev), Some(&next)) = (self.out.as_bytes().last(), text.as_bytes().first()) {
            if needs_space(prev, next, self.last_was_number) { self.out.push(' '); }
        }
        self.out.push_str(text);
        self.last_was_number = is_number;
    }
}

fn needs_space(p: u8, n: u8, prev_was_number: bool) -> bool {
    let pc = p as char;
    let nc = n as char;
    if is_idc(pc) && is_idc(nc) { return true; }
    if pc == '-' && nc == '-' { return true; }
    // Only need space between an actual number token and `.` to prevent `1.` ambiguity
    if prev_was_number && nc == '.' { return true; }
    if pc == '.' && nc == '.' { return true; }
    false
}

fn is_idc(c: char) -> bool { c.is_ascii_alphanumeric() || c == '_' }

// ── Recursive-descent minifier ─────────────────────────────────────────

struct Minifier {
    toks: Vec<Tok>,
    pos: usize,
    ren: Renamer,
    em: Emitter,
}

impl Minifier {
    fn new(toks: Vec<Tok>, no_self: bool) -> Self {
        Self { toks, pos: 0, ren: Renamer::new(no_self), em: Emitter::new() }
    }

    fn peek(&self) -> Option<&Tok> { self.toks.get(self.pos) }
    fn advance(&mut self) -> Option<Tok> {
        if self.pos < self.toks.len() { let t = self.toks[self.pos].clone(); self.pos += 1; Some(t) } else { None }
    }
    fn is_kw(&self, kw: &str) -> bool { matches!(self.peek(), Some(Tok::Keyword(k)) if k == kw) }
    fn is_punct(&self, p: &str) -> bool { matches!(self.peek(), Some(Tok::Punct(s)) if s == p) }
    fn is_ident(&self) -> bool { matches!(self.peek(), Some(Tok::Ident(_))) }
    fn eat_kw(&mut self, kw: &str) -> bool { if self.is_kw(kw) { self.advance(); true } else { false } }
    fn eat_punct(&mut self, p: &str) -> bool { if self.is_punct(p) { self.advance(); true } else { false } }

    fn run(&mut self) {
        self.block(&["eof"]);
    }

    /// Parse and emit a block of statements until we see a closing keyword.
    fn block(&mut self, closers: &[&str]) {
        loop {
            if self.pos >= self.toks.len() { break; }
            if let Some(Tok::Keyword(k)) = self.peek() {
                if closers.contains(&k.as_str()) { break; }
            }
            self.statement();
        }
    }

    fn statement(&mut self) {
        match self.peek() {
            None => { return; }
            Some(Tok::Keyword(k)) => {
                match k.as_str() {
                    "local" => self.stmt_local(),
                    "function" => self.stmt_function(),
                    "if" => self.stmt_if(),
                    "while" => self.stmt_while(),
                    "repeat" => self.stmt_repeat(),
                    "for" => self.stmt_for(),
                    "do" => self.stmt_do(),
                    "return" => self.stmt_return(),
                    "break" | "continue" => { let k = self.advance().unwrap(); if let Tok::Keyword(w) = k { self.em.emit(&w); } }
                    "goto" => { self.advance(); self.em.emit("goto"); if self.is_ident() { let t = self.advance().unwrap(); if let Tok::Ident(n) = t { self.em.emit(&n); } } }
                    _ => { self.expr_stat(); }
                }
            }
            _ => { self.expr_stat(); }
        }
    }

    // ── Statements ─────────────────────────────────────────────────────

    fn stmt_local(&mut self) {
        self.advance(); // local
        self.em.emit("local");

        if self.is_kw("function") {
            // local function NAME(...)
            self.advance();
            self.em.emit("function");
            if let Some(Tok::Ident(name)) = self.peek().cloned() {
                self.advance();
                self.ren.bind(&name);
                self.em.emit(&self.ren.resolve(&name));
            }
            self.func_body();
            return;
        }

        // local name, name, ... [= expr, expr, ...]
        let mut first = true;
        loop {
            if !first { self.em.emit(","); }
            if let Some(Tok::Ident(name)) = self.peek().cloned() {
                self.advance();
                self.ren.bind(&name);
                self.em.emit(&self.ren.resolve(&name));
                first = false;
            } else {
                break;
            }
            if !self.is_punct(",") { break; }
            self.advance(); // ,
        }

        if self.eat_punct("=") {
            self.em.emit("=");
            self.expr_list();
        }
    }

    fn stmt_function(&mut self) {
        self.advance(); // function
        self.em.emit("function");
        // function name[.name][:name](...)
        self.func_name();
        self.func_body();
    }

    fn func_name(&mut self) {
        // first part: resolve as it could be a local
        if let Some(Tok::Ident(name)) = self.peek().cloned() {
            self.advance();
            self.em.emit(&self.ren.resolve(&name));
        }
        // .name parts: don't rename (table fields)
        while self.is_punct(".") || self.is_punct(":") {
            let p = self.advance().unwrap();
            if let Tok::Punct(s) = p { self.em.emit(&s); }
            if let Some(Tok::Ident(name)) = self.peek().cloned() {
                self.advance();
                self.em.emit(&name); // table fields not renamed
            }
        }
    }

    fn func_body(&mut self) {
        self.ren.push();
        if self.eat_punct("(") {
            self.em.emit("(");
            self.param_list();
            // ) consumed by param_list
        }
        self.block(&["end"]);
        if self.eat_kw("end") { self.em.emit("end"); }
        self.ren.pop();
    }

    fn param_list(&mut self) {
        let mut first = true;
        loop {
            if self.is_punct(")") { self.advance(); self.em.emit(")"); return; }
            if !first { if self.eat_punct(",") { self.em.emit(","); } }
            if self.is_punct("...") {
                self.advance();
                self.em.emit("...");
            } else if let Some(Tok::Ident(name)) = self.peek().cloned() {
                self.advance();
                self.ren.bind(&name);
                self.em.emit(&self.ren.resolve(&name));
            } else {
                break;
            }
            first = false;
        }
        if self.eat_punct(")") { self.em.emit(")"); }
    }

    fn stmt_if(&mut self) {
        self.advance(); // if
        self.em.emit("if");
        self.expression(); // condition
        if self.eat_kw("then") { self.em.emit("then"); }
        self.block(&["else", "elseif", "end"]);
        loop {
            if self.eat_kw("elseif") {
                self.em.emit("elseif");
                self.expression();
                if self.eat_kw("then") { self.em.emit("then"); }
                self.block(&["else", "elseif", "end"]);
            } else if self.eat_kw("else") {
                self.em.emit("else");
                self.block(&["end"]);
            } else {
                break;
            }
        }
        if self.eat_kw("end") { self.em.emit("end"); }
    }

    fn stmt_while(&mut self) {
        self.advance(); self.em.emit("while");
        self.expression();
        if self.eat_kw("do") { self.em.emit("do"); }
        self.ren.push();
        self.block(&["end"]);
        if self.eat_kw("end") { self.em.emit("end"); }
        self.ren.pop();
    }

    fn stmt_repeat(&mut self) {
        self.advance(); self.em.emit("repeat");
        self.ren.push();
        self.block(&["until"]);
        if self.eat_kw("until") { self.em.emit("until"); }
        self.expression();
        self.ren.pop();
    }

    fn stmt_for(&mut self) {
        self.advance(); self.em.emit("for");
        self.ren.push();

        // Collect first name
        if let Some(Tok::Ident(name)) = self.peek().cloned() {
            self.advance();
            self.ren.bind(&name);
            self.em.emit(&self.ren.resolve(&name));
        }

        if self.is_punct("=") {
            // numeric for: for i = start, end [, step] do
            self.advance(); self.em.emit("=");
            self.expression();
            if self.eat_punct(",") { self.em.emit(","); self.expression(); }
            if self.eat_punct(",") { self.em.emit(","); self.expression(); }
        } else {
            // generic for: for a, b, ... in exprs do
            while self.eat_punct(",") {
                self.em.emit(",");
                if let Some(Tok::Ident(name)) = self.peek().cloned() {
                    self.advance();
                    self.ren.bind(&name);
                    self.em.emit(&self.ren.resolve(&name));
                }
            }
            if self.eat_kw("in") { self.em.emit("in"); }
            self.expr_list();
        }

        if self.eat_kw("do") { self.em.emit("do"); }
        self.block(&["end"]);
        if self.eat_kw("end") { self.em.emit("end"); }
        self.ren.pop();
    }

    fn stmt_do(&mut self) {
        self.advance(); self.em.emit("do");
        self.ren.push();
        self.block(&["end"]);
        if self.eat_kw("end") { self.em.emit("end"); }
        self.ren.pop();
    }

    fn stmt_return(&mut self) {
        self.advance(); self.em.emit("return");
        // return [exprs]
        if self.pos < self.toks.len() {
            if let Some(Tok::Keyword(k)) = self.peek() {
                if k == "end" || k == "else" || k == "elseif" || k == "until" { return; }
            }
            if self.is_punct(";") { return; }
            self.expr_list();
        }
    }

    fn expr_stat(&mut self) {
        // expression statement or assignment: expr [, expr] [= expr, expr]
        self.expression();
        // Check for , (multiple targets) or = or compound assign
        if self.is_punct(",") || self.is_punct("=") ||
           self.is_punct("+=") || self.is_punct("-=") || self.is_punct("*=") ||
           self.is_punct("/=") || self.is_punct("%=") || self.is_punct("^=") ||
           self.is_punct("..=")
        {
            while self.eat_punct(",") {
                self.em.emit(",");
                self.expression();
            }
            if let Some(Tok::Punct(p)) = self.peek().cloned() {
                if p == "=" || p == "+=" || p == "-=" || p == "*=" || p == "/=" ||
                   p == "%=" || p == "^=" || p == "..=" {
                    self.advance();
                    self.em.emit(&p);
                    self.expr_list();
                }
            }
        }
    }

    // ── Expressions ────────────────────────────────────────────────────

    fn expr_list(&mut self) {
        self.expression();
        while self.eat_punct(",") {
            self.em.emit(",");
            self.expression();
        }
    }

    fn expression(&mut self) {
        self.unary_expr();
        // Binary operators
        loop {
            if let Some(Tok::Keyword(k)) = self.peek() {
                match k.as_str() {
                    "and" | "or" => {
                        let k = self.advance().unwrap();
                        if let Tok::Keyword(w) = k { self.em.emit(&w); }
                        self.unary_expr();
                        continue;
                    }
                    _ => {}
                }
            }
            if let Some(Tok::Punct(p)) = self.peek() {
                match p.as_str() {
                    "+" | "-" | "*" | "/" | "%" | "^" | ".." | "//" |
                    "==" | "~=" | "<" | ">" | "<=" | ">=" | "<<" | ">>" |
                    "&" | "|" | "~" => {
                        let p = self.advance().unwrap();
                        if let Tok::Punct(s) = p { self.em.emit(&s); }
                        self.unary_expr();
                        continue;
                    }
                    _ => {}
                }
            }
            break;
        }
    }

    fn unary_expr(&mut self) {
        if let Some(Tok::Keyword(k)) = self.peek() {
            if k == "not" { self.advance(); self.em.emit("not"); self.unary_expr(); return; }
        }
        if let Some(Tok::Punct(p)) = self.peek() {
            if p == "-" || p == "#" || p == "~" {
                let p = self.advance().unwrap();
                if let Tok::Punct(s) = p { self.em.emit(&s); }
                self.unary_expr();
                return;
            }
        }
        self.primary_expr();
    }

    fn primary_expr(&mut self) {
        // Atom
        match self.peek().cloned() {
            None => {}
            Some(Tok::Number(n)) => { self.advance(); self.em.emit_number(&n); }
            Some(Tok::StringLit(s)) => { self.advance(); self.em.emit(&s); }
            Some(Tok::Keyword(k)) => {
                match k.as_str() {
                    "nil" | "true" | "false" => { self.advance(); self.em.emit(&k); }
                    "function" => {
                        // anonymous function
                        self.advance();
                        self.em.emit("function");
                        self.func_body();
                    }
                    "if" => {
                        // if-expression: if cond then expr [elseif cond then expr] else expr
                        self.advance(); self.em.emit("if");
                        self.expression();
                        if self.eat_kw("then") { self.em.emit("then"); }
                        self.expression();
                        while self.eat_kw("elseif") {
                            self.em.emit("elseif");
                            self.expression();
                            if self.eat_kw("then") { self.em.emit("then"); }
                            self.expression();
                        }
                        if self.eat_kw("else") { self.em.emit("else"); }
                        self.expression();
                    }
                    _ => {
                        // shouldn't happen but emit as identifier
                        self.advance(); self.em.emit(&k);
                    }
                }
            }
            Some(Tok::Ident(name)) => {
                self.advance();
                self.em.emit(&self.ren.resolve(&name));
            }
            Some(Tok::Punct(p)) => {
                match p.as_str() {
                    "(" => {
                        self.advance(); self.em.emit("(");
                        self.expression();
                        if self.eat_punct(")") { self.em.emit(")"); }
                    }
                    "{" => { self.table_constructor(); }
                    "..." => { self.advance(); self.em.emit("..."); }
                    _ => { self.advance(); self.em.emit(&p); }
                }
            }
        }

        // Suffixes: .field, :method(), [index], (args), {table}, "string"
        loop {
            match self.peek().cloned() {
                Some(Tok::Punct(p)) if p == "." => {
                    self.advance(); self.em.emit(".");
                    // field name — NOT renamed
                    if let Some(Tok::Ident(f)) = self.peek().cloned() {
                        self.advance(); self.em.emit(&f);
                    } else if let Some(Tok::Keyword(k)) = self.peek().cloned() {
                        // keyword used as field name (rare but valid in some contexts)
                        self.advance(); self.em.emit(&k);
                    }
                }
                Some(Tok::Punct(p)) if p == ":" => {
                    self.advance(); self.em.emit(":");
                    // method name — NOT renamed
                    if let Some(Tok::Ident(m)) = self.peek().cloned() {
                        self.advance(); self.em.emit(&m);
                    } else if let Some(Tok::Keyword(k)) = self.peek().cloned() {
                        self.advance(); self.em.emit(&k);
                    }
                    // call args
                    if self.is_punct("(") {
                        self.advance(); self.em.emit("(");
                        self.call_args();
                    } else if self.is_punct("{") {
                        self.table_constructor();
                    } else if let Some(Tok::StringLit(_)) = self.peek() {
                        let s = self.advance().unwrap();
                        if let Tok::StringLit(v) = s { self.em.emit(&v); }
                    }
                }
                Some(Tok::Punct(p)) if p == "[" => {
                    self.advance(); self.em.emit("[");
                    self.expression();
                    if self.eat_punct("]") { self.em.emit("]"); }
                }
                Some(Tok::Punct(p)) if p == "(" => {
                    self.advance(); self.em.emit("(");
                    self.call_args();
                }
                Some(Tok::Punct(p)) if p == "{" => {
                    self.table_constructor();
                }
                Some(Tok::StringLit(s)) => {
                    // f"string" call syntax
                    self.advance(); self.em.emit(&s);
                }
                _ => break,
            }
        }
    }

    fn call_args(&mut self) {
        if self.is_punct(")") { self.advance(); self.em.emit(")"); return; }
        self.expr_list();
        if self.eat_punct(")") { self.em.emit(")"); }
    }

    fn table_constructor(&mut self) {
        self.advance(); // {
        self.em.emit("{");
        loop {
            if self.is_punct("}") { break; }

            if self.is_punct("[") {
                // [expr] = expr
                self.advance(); self.em.emit("[");
                self.expression();
                if self.eat_punct("]") { self.em.emit("]"); }
                if self.eat_punct("=") { self.em.emit("="); }
                self.expression();
            } else if let Some(Tok::Ident(_)) = self.peek() {
                // Check if it's name = expr or just expr
                let saved = self.pos;
                self.advance(); // ident
                if self.is_punct("=") {
                    // name = expr — field name, don't rename
                    self.pos = saved;
                    let name = if let Some(Tok::Ident(n)) = self.peek().cloned() { self.advance(); n } else { String::new() };
                    self.em.emit(&name);
                    self.advance(); // =
                    self.em.emit("=");
                    self.expression();
                } else {
                    // Just an expression starting with ident
                    self.pos = saved;
                    self.expression();
                }
            } else {
                self.expression();
            }

            if self.eat_punct(",") { self.em.emit(","); }
            else if self.eat_punct(";") { self.em.emit(";"); }
            else if !self.is_punct("}") { break; }
        }
        if self.eat_punct("}") { self.em.emit("}"); }
    }
}

// ── Public API ─────────────────────────────────────────────────────────

pub fn minify(source: &str, no_self: bool) -> String {
    let tokens = tokenize(source);
    let mut m = Minifier::new(tokens, no_self);
    m.run();
    m.em.out.trim().to_string()
}
