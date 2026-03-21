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

        // E001: Cannot instantiate abstract class
        diagnostics.extend(rules::check_abstract_instantiation(file, self.symbol_table));

        // E002: Non-abstract class must implement all inherited abstract methods (transitive)
        diagnostics.extend(rules::check_abstract_methods(file, self.symbol_table));

        // E003: Class with abstract methods must be declared abstract
        diagnostics.extend(rules::check_abstract_class_has_abstract_methods(file, self.symbol_table));

        // E004: Cannot extend sealed class from different file
        diagnostics.extend(rules::check_sealed_inheritance(file, self.symbol_table));

        // E006: super used outside of a class with a parent
        diagnostics.extend(rules::check_super_usage(file, self.symbol_table));

        // E007: override specified but no parent method exists + signature validation
        diagnostics.extend(rules::check_override_validity(file, self.symbol_table));
        diagnostics.extend(rules::check_override_signatures(file, self.symbol_table));

        // E009/E010: Access modifier enforcement (private/protected)
        diagnostics.extend(rules::check_access_modifiers(file, self.symbol_table));

        // E011: Readonly field assignment outside constructor
        diagnostics.extend(rules::check_readonly_assignments(file, self.symbol_table));

        // E012: Interface conformance (methods + fields + signatures)
        diagnostics.extend(rules::check_interface_conformance(file, self.symbol_table));

        // E013: Duplicate enum entries
        diagnostics.extend(rules::check_duplicate_enum_entries(file, self.symbol_table));

        // E014: Mixed auto-increment and string values in enum
        diagnostics.extend(rules::check_enum_mixed_values(file, self.symbol_table));

        // E015: Static method cannot reference self
        diagnostics.extend(rules::check_static_self_usage(file, self.symbol_table));

        // E016: Constructor must not return a value
        diagnostics.extend(rules::check_constructor_return(file, self.symbol_table));

        // E017: Duplicate class member
        diagnostics.extend(rules::check_duplicate_members(file, self.symbol_table));

        // E018: Type mismatch (assignments, returns, operators, function args)
        diagnostics.extend(rules::check_type_mismatches(file, self.symbol_table));

        // E019: instanceof right-hand side must be a class name
        diagnostics.extend(rules::check_instanceof_usage(file, self.symbol_table));

        // E020: Multiple constructors per class
        diagnostics.extend(rules::check_multiple_constructors(file, self.symbol_table));

        // Union type member access
        diagnostics.extend(rules::check_union_member_access(file, self.symbol_table));

        // Import shadowing
        diagnostics.extend(rules::check_import_shadowing(file, self.symbol_table));

        // E021: yield outside generator function
        diagnostics.extend(rules::check_yield_outside_generator(file, self.symbol_table));

        // E022: await outside async function (top-level await is allowed)
        diagnostics.extend(rules::check_await_outside_async(file, self.symbol_table));

        // E023: Reserved built-in type names (Promise)
        diagnostics.extend(rules::check_reserved_names(file, self.symbol_table));

        diagnostics
    }
}
