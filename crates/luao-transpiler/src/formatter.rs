pub fn format_lua(source: &str) -> String {
    source.to_string()
}

/// Minifies Lua source: renames local variables to short names and strips whitespace/comments.
pub fn minify_lua(source: &str, no_self: bool) -> String {
    crate::minifier::minify(source, no_self)
}

/// Minifies with promoted globals that should be renamed like locals.
/// Returns `(minified_source, rename_map)`.
pub fn minify_lua_with_globals(source: &str, no_self: bool, promoted_globals: &[String]) -> (String, Vec<(String, String)>) {
    crate::minifier::minify_with_globals(source, no_self, promoted_globals)
}
