pub fn format_lua(source: &str) -> String {
    match full_moon::parse(source) {
        Ok(ast) => ast.to_string(),
        Err(_) => source.to_string(),
    }
}

/// Minifies Lua source: renames local variables to short names and strips whitespace/comments.
pub fn minify_lua(source: &str, no_self: bool) -> String {
    crate::minifier::minify_with_options(source, no_self)
}
