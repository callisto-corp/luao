pub mod class_emitter;
pub mod emitter;
pub mod enum_emitter;
pub mod expression_emitter;
pub mod formatter;
pub mod runtime;
pub mod statement_emitter;

pub use emitter::Emitter;

pub fn transpile(source: &str) -> Result<String, Vec<String>> {
    let (ast, parse_errors) = luao_parser::parse(source);
    if !parse_errors.is_empty() {
        return Err(parse_errors.iter().map(|e| e.to_string()).collect());
    }
    let mut resolver = luao_resolver::Resolver::new();
    let symbol_table = resolver.resolve(&ast);
    let checker = luao_checker::Checker::new(&symbol_table);
    let diagnostics = checker.check(&ast);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == luao_checker::DiagnosticSeverity::Error)
        .map(|d| d.to_string())
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut emitter = Emitter::new(symbol_table);
    emitter.emit(&ast);
    let lua_source = emitter.output();
    Ok(formatter::format_lua(&lua_source))
}
