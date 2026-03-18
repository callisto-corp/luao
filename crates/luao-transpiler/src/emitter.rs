use luao_parser::{SourceFile, Statement};
use luao_resolver::SymbolTable;

use crate::class_emitter;
use crate::enum_emitter;
use crate::runtime;
use crate::statement_emitter;

pub struct Emitter {
    pub(crate) output: String,
    pub(crate) indent_level: usize,
    pub(crate) symbol_table: SymbolTable,
    pub(crate) needs_instanceof: bool,
    pub(crate) needs_enum_freeze: bool,
    pub(crate) current_class: Option<String>,
    pub(crate) current_class_parent: Option<String>,
}

impl Emitter {
    pub fn new(symbol_table: SymbolTable) -> Self {
        Self {
            output: String::new(),
            indent_level: 0,
            symbol_table,
            needs_instanceof: false,
            needs_enum_freeze: false,
            current_class: None,
            current_class_parent: None,
        }
    }

    pub fn emit(&mut self, file: &SourceFile) {
        for stmt in &file.statements {
            self.emit_statement(stmt);
        }
    }

    pub fn output(mut self) -> String {
        let mut preamble = String::new();
        if self.needs_instanceof {
            preamble.push_str(runtime::INSTANCEOF_FN);
            preamble.push('\n');
            preamble.push('\n');
        }
        if self.needs_enum_freeze {
            preamble.push_str(runtime::ENUM_FREEZE_FN);
            preamble.push('\n');
            preamble.push('\n');
        }
        if !preamble.is_empty() {
            preamble.push_str(&self.output);
            self.output = preamble;
        }
        self.output
    }

    pub fn emit_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::ClassDecl(class) => {
                class_emitter::emit_class(self, class);
            }
            Statement::EnumDecl(enum_decl) => {
                enum_emitter::emit_enum(self, enum_decl);
            }
            Statement::InterfaceDecl(_) => {}
            _ => {
                statement_emitter::emit_statement(self, stmt);
            }
        }
    }

    pub fn write(&mut self, s: &str) {
        self.output.push_str(s);
    }

    pub fn writeln(&mut self, s: &str) {
        self.write_indent();
        self.output.push_str(s);
        self.output.push('\n');
    }

    pub fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.output.push_str("    ");
        }
    }

    pub fn indent(&mut self) {
        self.indent_level += 1;
    }

    pub fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    pub fn newline(&mut self) {
        self.output.push('\n');
    }

    pub fn emit_block(&mut self, block: &luao_parser::Block) {
        self.indent();
        for stmt in &block.statements {
            self.emit_statement(stmt);
        }
        self.dedent();
    }

    pub fn emit_params(&mut self, params: &[luao_parser::Parameter]) -> String {
        params
            .iter()
            .map(|p| {
                if p.is_vararg {
                    "...".to_string()
                } else {
                    p.name.name.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}
