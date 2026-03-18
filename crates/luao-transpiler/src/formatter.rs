pub fn format_lua(source: &str) -> String {
    source.to_string()
}

/// Minifies Lua source: renames local variables to short names and strips whitespace/comments.
pub fn minify_lua(source: &str, no_self: bool) -> String {
    crate::minifier::minify(source, no_self)
}
