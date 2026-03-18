use luao_parser::SourceFile;
use luao_resolver::SymbolTable;

use crate::diagnostic::Diagnostic;
use crate::rules;

pub struct Checker<'a> {
    symbol_table: &'a SymbolTable,
}

impl<'a> Checker<'a> {
    pub fn new(symbol_table: &'a SymbolTable) -> Self {
        Self { symbol_table }
    }

    pub fn check(&self, file: &SourceFile) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        diagnostics.extend(rules::check_abstract_instantiation(file, self.symbol_table));
        diagnostics.extend(rules::check_abstract_methods(file, self.symbol_table));
        diagnostics.extend(rules::check_sealed_inheritance(file, self.symbol_table));
        diagnostics.extend(rules::check_interface_conformance(file, self.symbol_table));
        diagnostics.extend(rules::check_readonly_assignments(file, self.symbol_table));
        diagnostics.extend(rules::check_access_modifiers(file, self.symbol_table));
        diagnostics.extend(rules::check_override_validity(file, self.symbol_table));
        diagnostics.extend(rules::check_super_usage(file, self.symbol_table));
        diagnostics.extend(rules::check_union_member_access(file, self.symbol_table));
        diagnostics.extend(rules::check_import_shadowing(file, self.symbol_table));
        diagnostics
    }
}
