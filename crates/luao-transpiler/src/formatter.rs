pub fn format_lua(source: &str) -> String {
    match full_moon::parse(source) {
        Ok(ast) => ast.to_string(),
        Err(_) => source.to_string(),
    }
}
