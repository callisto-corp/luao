pub fn format_lua(source: &str) -> String {
    match full_moon::parse(source) {
        Ok(ast) => ast.to_string(),
        Err(_) => source.to_string(),
    }
}

/// Minifies Lua source by stripping indentation and blank lines.
pub fn minify_lua(source: &str) -> String {
    source
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
