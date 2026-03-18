use dashmap::DashMap;
use luao_checker::Checker;
use luao_lexer::Lexer;
use luao_parser::SourceFile;
use luao_resolver::{Resolver, SymbolTable};
use tower_lsp::lsp_types::Url;

pub struct DocumentState {
    pub content: String,
    pub version: i32,
    pub ast: Option<SourceFile>,
    pub symbol_table: Option<SymbolTable>,
    pub diagnostics: Vec<luao_checker::Diagnostic>,
}

pub struct DocumentStore {
    documents: DashMap<Url, DocumentState>,
}

impl DocumentStore {
    pub fn new() -> Self {
        Self {
            documents: DashMap::new(),
        }
    }

    pub fn open(&self, uri: Url, content: String, version: i32) {
        let mut state = DocumentState {
            content,
            version,
            ast: None,
            symbol_table: None,
            diagnostics: Vec::new(),
        };
        Self::parse_document(&mut state);
        self.documents.insert(uri, state);
    }

    pub fn update(&self, uri: &Url, content: String, version: i32) {
        if let Some(mut entry) = self.documents.get_mut(uri) {
            entry.content = content;
            entry.version = version;
            Self::parse_document(&mut entry);
        }
    }

    pub fn close(&self, uri: &Url) {
        self.documents.remove(uri);
    }

    pub fn get(&self, uri: &Url) -> Option<dashmap::mapref::one::Ref<'_, Url, DocumentState>> {
        self.documents.get(uri)
    }

    pub fn reparse(&self, uri: &Url) {
        if let Some(mut entry) = self.documents.get_mut(uri) {
            Self::parse_document(&mut entry);
        }
    }

    fn parse_document(state: &mut DocumentState) {
        let tokens = Lexer::new(&state.content).tokenize();
        let (ast, parse_errors) = luao_parser::parse(&state.content);

        let mut resolver = Resolver::new();
        let symbol_table = resolver.resolve(&ast);

        let checker = Checker::new(&symbol_table);
        let mut diagnostics = checker.check(&ast);

        for err in &parse_errors {
            diagnostics.push(luao_checker::Diagnostic::error(
                err.message.clone(),
                err.span,
                "parse_error",
            ));
        }

        let _ = tokens;
        state.ast = Some(ast);
        state.symbol_table = Some(symbol_table);
        state.diagnostics = diagnostics;
    }
}

impl Default for DocumentStore {
    fn default() -> Self {
        Self::new()
    }
}
