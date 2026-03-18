//! Lua minifier — renames local variables to short names and strips whitespace.
//!
//! Two-phase approach:
//! 1. Scope analysis: walk the AST to build a scope tree mapping each local
//!    binding to a short generated name, respecting shadowing and nested scopes.
//! 2. Token rewrite: walk the AST with VisitorMut, renaming every identifier
//!    that refers to a local binding and stripping trivia (whitespace/comments).

use std::collections::{HashMap, HashSet};

use full_moon::ast::{self, Block, Expression, FunctionBody, LastStmt, Parameter, Prefix, Stmt, Var};
use full_moon::tokenizer::{Token, TokenReference, TokenType};
use full_moon::visitors::VisitorMut;

// ── Name generation ────────────────────────────────────────────────────

const FIRST: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_";
const REST: &[u8]  = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789";

const LUA_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for",
    "function", "if", "in", "local", "nil", "not", "or", "repeat",
    "return", "then", "true", "until", "while",
    // lua 5.2+
    "goto", "continue",
];

fn index_to_name(mut idx: usize) -> String {
    let fc = FIRST.len();
    let rc = REST.len();
    if idx < fc {
        return String::from(FIRST[idx] as char);
    }
    idx -= fc;
    let mut len: u32 = 2;
    let mut count = fc * rc;
    while idx >= count {
        idx -= count;
        len += 1;
        count *= rc;
    }
    let mut name = String::with_capacity(len as usize);
    let rp = rc.pow(len - 1);
    name.push(FIRST[idx / rp] as char);
    idx %= rp;
    for i in (0..len - 1).rev() {
        let d = rc.pow(i);
        name.push(REST[idx / d] as char);
        idx %= d;
    }
    name
}

// ── Scope tracking ─────────────────────────────────────────────────────

/// A scope frame: tracks which names are local bindings introduced in this scope,
/// and what short name they map to.
struct Scope {
    /// original name → short name
    renames: HashMap<String, String>,
}

/// Walks the Lua AST and builds a rename table for every local binding.
/// Returns a flat map: (original_name, declaration_line) → short_name is too fragile;
/// instead we return a map keyed on the token's byte position for precision.
struct ScopeAnalyzer {
    scopes: Vec<Scope>,
    /// Maps token start byte offset → short name.  This is the most precise way
    /// to target exactly the right token when multiple locals share the same name
    /// in different scopes.
    rename_map: HashMap<(usize, usize), String>,
    /// Global counter for generating short names within each scope level.
    name_counter: usize,
    /// Names that are known globals (never rename these).
    globals: HashSet<String>,
    /// When true, `self` is treated as a normal renameable local.
    no_self: bool,
}

impl ScopeAnalyzer {
    fn new(no_self: bool) -> Self {
        Self {
            scopes: vec![Scope { renames: HashMap::new() }], // root scope
            rename_map: HashMap::new(),
            name_counter: 0,
            globals: HashSet::new(),
            no_self,
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope { renames: HashMap::new() });
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn next_short_name(&mut self) -> String {
        loop {
            let name = index_to_name(self.name_counter);
            self.name_counter += 1;
            if !LUA_KEYWORDS.contains(&name.as_str()) && !self.globals.contains(&name) {
                return name;
            }
        }
    }

    /// Register a local binding in the current scope.
    fn bind_local(&mut self, token: &TokenReference) {
        let name = get_token_name(token);
        if name == "..." { return; }
        if name == "self" && !self.no_self { return; }
        let short = self.next_short_name();
        let pos = token_pos(token);
        self.rename_map.insert(pos, short.clone());
        if let Some(scope) = self.scopes.last_mut() {
            scope.renames.insert(name, short);
        }
    }

    /// Look up the rename for a variable reference.
    fn lookup(&self, name: &str) -> Option<&str> {
        for scope in self.scopes.iter().rev() {
            if let Some(short) = scope.renames.get(name) {
                return Some(short);
            }
        }
        None
    }

    /// Resolve and record a reference usage.
    fn resolve_ref(&mut self, token: &TokenReference) {
        let name = get_token_name(token);
        if name == "self" && !self.no_self { return; }
        if let Some(short) = self.lookup(&name) {
            let pos = token_pos(token);
            self.rename_map.insert(pos, short.to_string());
        }
    }
}

fn get_token_name(t: &TokenReference) -> String {
    match t.token().token_type() {
        TokenType::Identifier { identifier } => identifier.to_string(),
        _ => t.to_string().trim().to_string(),
    }
}

fn token_pos(t: &TokenReference) -> (usize, usize) {
    let p = t.token().start_position();
    (p.bytes(), p.line())
}

// ── Phase 1: Scope analysis via Visitor ────────────────────────────────

impl ScopeAnalyzer {
    fn analyze_block(&mut self, block: &Block) {
        for stmt in block.stmts() {
            self.analyze_stmt(stmt);
        }
        if let Some(last) = block.last_stmt() {
            self.analyze_last_stmt(last);
        }
    }

    fn analyze_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::LocalAssignment(la) => {
                // First analyze the RHS (before the locals are in scope)
                for expr in la.expressions().iter() {
                    self.analyze_expr(expr);
                }
                // Then bind the locals
                for pair in la.names().pairs() {
                    self.bind_local(pair.value());
                }
            }
            Stmt::LocalFunction(lf) => {
                // The function name is local in the current scope
                self.bind_local(lf.name());
                // Function body has its own scope
                self.analyze_function_body(lf.body());
            }
            Stmt::FunctionDeclaration(fd) => {
                // Resolve the function name parts as references
                let name = fd.name();
                for pair in name.names().pairs() {
                    self.resolve_ref(pair.value());
                }
                if let Some(method) = name.method_name() {
                    // method name is after `:`, don't rename
                    let _ = method;
                }
                self.analyze_function_body(fd.body());
            }
            Stmt::Assignment(assign) => {
                for var in assign.variables().iter() {
                    self.analyze_var(var);
                }
                for expr in assign.expressions().iter() {
                    self.analyze_expr(expr);
                }
            }
            Stmt::FunctionCall(call) => {
                self.analyze_function_call(call);
            }
            Stmt::Do(do_stmt) => {
                self.push_scope();
                self.analyze_block(do_stmt.block());
                self.pop_scope();
            }
            Stmt::If(if_stmt) => {
                self.analyze_expr(if_stmt.condition());
                self.push_scope();
                self.analyze_block(if_stmt.block());
                self.pop_scope();
                if let Some(else_ifs) = if_stmt.else_if() {
                    for else_if in else_ifs {
                        self.analyze_expr(else_if.condition());
                        self.push_scope();
                        self.analyze_block(else_if.block());
                        self.pop_scope();
                    }
                }
                if let Some(else_block) = if_stmt.else_block() {
                    self.push_scope();
                    self.analyze_block(else_block);
                    self.pop_scope();
                }
            }
            Stmt::While(while_stmt) => {
                self.analyze_expr(while_stmt.condition());
                self.push_scope();
                self.analyze_block(while_stmt.block());
                self.pop_scope();
            }
            Stmt::Repeat(repeat) => {
                self.push_scope();
                self.analyze_block(repeat.block());
                self.analyze_expr(repeat.until());
                self.pop_scope();
            }
            Stmt::NumericFor(nf) => {
                self.analyze_expr(nf.start());
                self.analyze_expr(nf.end());
                if let Some(step) = nf.step() {
                    self.analyze_expr(step);
                }
                self.push_scope();
                self.bind_local(nf.index_variable());
                self.analyze_block(nf.block());
                self.pop_scope();
            }
            Stmt::GenericFor(gf) => {
                for expr in gf.expressions().iter() {
                    self.analyze_expr(expr);
                }
                self.push_scope();
                for pair in gf.names().pairs() {
                    self.bind_local(pair.value());
                }
                self.analyze_block(gf.block());
                self.pop_scope();
            }
            _ => {}
        }
    }

    fn analyze_last_stmt(&mut self, stmt: &LastStmt) {
        if let LastStmt::Return(ret) = stmt {
            for expr in ret.returns().iter() {
                self.analyze_expr(expr);
            }
        }
    }

    fn analyze_function_body(&mut self, body: &FunctionBody) {
        self.push_scope();
        for pair in body.parameters().pairs() {
            match pair.value() {
                Parameter::Name(name) => self.bind_local(name),
                Parameter::Ellipsis(_) => {}
                _ => {}
            }
        }
        self.analyze_block(body.block());
        self.pop_scope();
    }

    fn analyze_var(&mut self, var: &Var) {
        match var {
            Var::Name(name) => self.resolve_ref(name),
            Var::Expression(expr) => {
                self.analyze_prefix(expr.prefix());
                for suffix in expr.suffixes() {
                    self.analyze_suffix(suffix);
                }
            }
            _ => {}
        }
    }

    fn analyze_prefix(&mut self, prefix: &Prefix) {
        match prefix {
            Prefix::Name(name) => self.resolve_ref(name),
            Prefix::Expression(expr) => self.analyze_expr(expr),
            _ => {}
        }
    }

    fn analyze_suffix(&mut self, suffix: &ast::Suffix) {
        match suffix {
            ast::Suffix::Call(call) => match call {
                ast::Call::AnonymousCall(args) => self.analyze_function_args(args),
                ast::Call::MethodCall(mc) => self.analyze_function_args(mc.args()),
                _ => {}
            },
            ast::Suffix::Index(idx) => match idx {
                ast::Index::Brackets { expression, .. } => self.analyze_expr(expression),
                ast::Index::Dot { name, .. } => { let _ = name; } // field access, not a var ref
                _ => {}
            },
            _ => {}
        }
    }

    fn analyze_function_args(&mut self, args: &ast::FunctionArgs) {
        match args {
            ast::FunctionArgs::Parentheses { arguments, .. } => {
                for expr in arguments.iter() {
                    self.analyze_expr(expr);
                }
            }
            ast::FunctionArgs::String(_) => {}
            ast::FunctionArgs::TableConstructor(tc) => self.analyze_table_constructor(tc),
            _ => {}
        }
    }

    fn analyze_function_call(&mut self, call: &ast::FunctionCall) {
        self.analyze_prefix(call.prefix());
        for suffix in call.suffixes() {
            self.analyze_suffix(suffix);
        }
    }

    fn analyze_expr(&mut self, expr: &Expression) {
        match expr {
            Expression::BinaryOperator { lhs, rhs, .. } => {
                self.analyze_expr(lhs);
                self.analyze_expr(rhs);
            }
            Expression::Parentheses { expression, .. } => {
                self.analyze_expr(expression);
            }
            Expression::UnaryOperator { expression, .. } => {
                self.analyze_expr(expression);
            }
            Expression::Function(f) => {
                self.analyze_function_body(&f.1);
            }
            Expression::FunctionCall(call) => {
                self.analyze_function_call(call);
            }
            Expression::TableConstructor(tc) => {
                self.analyze_table_constructor(tc);
            }
            Expression::Number(_) | Expression::String(_) | Expression::Symbol(_) => {}
            Expression::Var(var) => {
                self.analyze_var(var);
            }
            _ => {}
        }
    }

    fn analyze_table_constructor(&mut self, tc: &ast::TableConstructor) {
        for field in tc.fields().iter() {
            match field {
                ast::Field::ExpressionKey { key, value, .. } => {
                    self.analyze_expr(key);
                    self.analyze_expr(value);
                }
                ast::Field::NameKey { value, .. } => {
                    self.analyze_expr(value);
                }
                ast::Field::NoKey(expr) => {
                    self.analyze_expr(expr);
                }
                _ => {}
            }
        }
    }
}

// ── Phase 2: Token rewrite via VisitorMut ──────────────────────────────

struct Renamer {
    rename_map: HashMap<(usize, usize), String>,
}

impl Renamer {
    fn rename_token(&self, token: TokenReference) -> TokenReference {
        let pos = token_pos(&token);
        if let Some(new_name) = self.rename_map.get(&pos) {
            let new_token = Token::new(TokenType::Identifier {
                identifier: new_name.as_str().into(),
            });
            let leading = minimize_trivia(token.leading_trivia().collect());
            let trailing = minimize_trivia(token.trailing_trivia().collect());
            TokenReference::new(leading, new_token, trailing)
        } else {
            strip_trivia_from_token(token)
        }
    }
}

impl VisitorMut for Renamer {
    fn visit_token_reference(&mut self, token: TokenReference) -> TokenReference {
        let pos = token_pos(&token);
        if self.rename_map.contains_key(&pos) {
            self.rename_token(token)
        } else {
            strip_trivia_from_token(token)
        }
    }
}

// ── Trivia stripping ───────────────────────────────────────────────────

fn strip_trivia_from_token(token: TokenReference) -> TokenReference {
    let leading = minimize_trivia(token.leading_trivia().collect());
    let trailing = minimize_trivia(token.trailing_trivia().collect());
    TokenReference::new(leading, token.token().clone(), trailing)
}

/// Keep a single space if any whitespace existed (needed for token separation).
/// Post-processing will remove unnecessary spaces.
fn minimize_trivia(trivia: Vec<&Token>) -> Vec<Token> {
    let has_whitespace = trivia.iter().any(|t| {
        matches!(t.token_type(), TokenType::Whitespace { .. })
    });
    if has_whitespace {
        vec![Token::new(TokenType::Whitespace { characters: " ".into() })]
    } else {
        Vec::new()
    }
}

// ── Public API ─────────────────────────────────────────────────────────

/// Minify Lua source: rename locals to short names, strip whitespace/comments.
pub fn minify(source: &str) -> String {
    minify_with_options(source, false)
}

/// Minify with options. When `no_self` is true, `self` is treated as a normal
/// local variable and will be renamed.
pub fn minify_with_options(source: &str, no_self: bool) -> String {
    let ast = match full_moon::parse(source) {
        Ok(ast) => ast,
        Err(_) => return source.to_string(),
    };

    // Phase 1: analyze scopes
    let mut analyzer = ScopeAnalyzer::new(no_self);
    analyzer.analyze_block(ast.nodes());

    // Phase 2: rename + strip trivia
    let mut renamer = Renamer {
        rename_map: analyzer.rename_map,
    };

    let ast = renamer.visit_ast(ast);
    let raw = ast.to_string();

    // Post-process: insert spaces only where two adjacent tokens would merge.
    // Two alphanumeric/underscore chars next to each other need a space.
    // Also need space after keywords before `(` in some cases like `function(`.
    // And between `-` `-` to avoid creating `--` (comment).
    let bytes = raw.as_bytes();
    let mut result = String::with_capacity(raw.len());

    for (i, &b) in bytes.iter().enumerate() {
        let ch = b as char;
        if ch.is_ascii_whitespace() {
            // Only emit a space if the previous and next non-whitespace chars
            // would merge into an invalid token
            if let (Some(&prev), Some(next)) = (result.as_bytes().last(), peek_nonws(bytes, i + 1)) {
                if needs_space(prev, next) {
                    result.push(' ');
                }
            }
        } else {
            result.push(ch);
        }
    }

    result.trim().to_string()
}

/// Peek ahead past whitespace to find the next non-whitespace byte.
fn peek_nonws(bytes: &[u8], start: usize) -> Option<u8> {
    for &b in &bytes[start..] {
        if !(b as char).is_ascii_whitespace() {
            return Some(b);
        }
    }
    None
}

/// Returns true if a space is needed between two adjacent characters.
fn needs_space(prev: u8, next: u8) -> bool {
    let p = prev as char;
    let n = next as char;

    // Two identifier/keyword chars would merge
    if is_ident_char(p) && is_ident_char(n) {
        return true;
    }

    // Prevent `--` (comment) from two minus signs
    if p == '-' && n == '-' {
        return true;
    }

    // Prevent `..` number ambiguity: `1 ..` vs `1..` (1.. could be parsed as 1. followed by .)
    // Actually `..` is the concat operator, but `1..` is ambiguous with number `1.`
    if p == '.' && n == '.' {
        // Only if preceded by a digit context — but safer to just leave it
        return false;
    }

    // Number followed by identifier start (e.g. `0x` is valid, but `3e` could be ambiguous)
    // This is very rare in practice since our output is controlled

    false
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
