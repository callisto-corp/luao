pub mod bundler;
pub mod class_emitter;
pub mod emitter;
pub mod enum_emitter;
pub mod expression_emitter;
pub mod formatter;
pub mod mangler;
pub mod minifier;
pub mod runtime;
pub mod statement_emitter;

pub use emitter::Emitter;

#[derive(Debug, Clone, Default)]
pub struct TranspileOptions {
    pub minify: bool,
    pub mangle: bool,
    pub no_self: bool,
}

pub fn transpile(source: &str) -> Result<String, Vec<String>> {
    transpile_with_options(source, &TranspileOptions::default())
}

pub fn transpile_with_options(
    source: &str,
    options: &TranspileOptions,
) -> Result<String, Vec<String>> {
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
    let mangler = if options.mangle {
        Some(mangler::Mangler::new())
    } else {
        None
    };
    let mut emitter = Emitter::new(symbol_table, mangler);
    emitter.no_self = options.no_self;
    emitter.emit(&ast);
    let lua_source = emitter.output();
    if options.minify {
        Ok(formatter::minify_lua(&lua_source, options.no_self))
    } else {
        Ok(formatter::format_lua(&lua_source))
    }
}
