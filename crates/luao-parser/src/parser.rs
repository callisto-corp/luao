use crate::ast::*;
use crate::error::*;
use luao_lexer::{Lexer, Span, Token, TokenKind};
use smol_str::SmolStr;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<ParseError>,
}

impl Parser {
    pub fn new(source: &str) -> Self {
        let tokens = Lexer::new(source).tokenize();
        Self {
            tokens,
            pos: 0,
            errors: Vec::new(),
        }
    }

    pub fn parse(mut self) -> (SourceFile, Vec<ParseError>) {
        let start = self.current_span();
        let mut statements = Vec::new();

        while !self.is_at_end() {
            match self.parse_statement() {
                Ok(stmt) => statements.push(stmt),
                Err(e) => {
                    self.errors.push(e);
                    self.synchronize();
                }
            }
        }

        let end = self.current_span();
        let file = SourceFile {
            statements,
            span: start.merge(end),
        };
        (file, self.errors)
    }

    fn synchronize(&mut self) {
        self.advance();
        while !self.is_at_end() {
            match self.current().kind {
                TokenKind::Class
                | TokenKind::Interface
                | TokenKind::Enum
                | TokenKind::Function
                | TokenKind::Local
                | TokenKind::If
                | TokenKind::While
                | TokenKind::For
                | TokenKind::Repeat
                | TokenKind::Return
                | TokenKind::Do
                | TokenKind::Switch
                | TokenKind::Abstract
                | TokenKind::Sealed => return,
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn parse_statement(&mut self) -> ParseResult<Statement> {
        self.skip_semicolons();
        if self.is_at_end() {
            return Err(self.error("unexpected end of file", ParseErrorKind::UnexpectedEof));
        }

        match self.current().kind {
            TokenKind::Class if self.peek_ahead(1).kind == TokenKind::Identifier || self.peek_ahead(1).kind.is_contextual_keyword() => self.parse_class_decl(false, false, false),
            TokenKind::Abstract if self.peek_ahead(1).kind == TokenKind::Class => {
                self.advance();
                self.parse_class_decl(true, false, false)
            }
            TokenKind::Sealed if self.peek_ahead(1).kind == TokenKind::Class => {
                self.advance();
                self.parse_class_decl(false, true, false)
            }
            TokenKind::Extern if matches!(self.peek_ahead(1).kind, TokenKind::Class | TokenKind::Abstract | TokenKind::Sealed | TokenKind::Interface | TokenKind::Enum) => {
                let start = self.current_span();
                self.advance();
                match self.current().kind {
                    TokenKind::Class => self.parse_class_decl(false, false, true),
                    TokenKind::Abstract => {
                        self.advance();
                        if self.check(TokenKind::Class) {
                            self.parse_class_decl(true, false, true)
                        } else {
                            Err(self.error_at(
                                start,
                                "expected 'class' after 'extern abstract'",
                                ParseErrorKind::InvalidStatement,
                            ))
                        }
                    }
                    TokenKind::Sealed => {
                        self.advance();
                        if self.check(TokenKind::Class) {
                            self.parse_class_decl(false, true, true)
                        } else {
                            Err(self.error_at(
                                start,
                                "expected 'class' after 'extern sealed'",
                                ParseErrorKind::InvalidStatement,
                            ))
                        }
                    }
                    TokenKind::Interface => self.parse_interface_decl_with_extern(true),
                    TokenKind::Enum => self.parse_enum_decl_with_extern(true),
                    _ => Err(self.error_at(
                        start,
                        "expected 'class', 'interface', or 'enum' after 'extern'",
                        ParseErrorKind::InvalidStatement,
                    )),
                }
            }
            TokenKind::Interface if self.peek_ahead(1).kind == TokenKind::Identifier || self.peek_ahead(1).kind.is_contextual_keyword() => self.parse_interface_decl_with_extern(false),
            TokenKind::Enum => self.parse_enum_decl_with_extern(false),
            TokenKind::Import if self.peek_ahead(1).kind == TokenKind::Identifier || self.peek_ahead(1).kind.is_contextual_keyword() => self.parse_import_decl(),
            TokenKind::Export if matches!(self.peek_ahead(1).kind, TokenKind::Local | TokenKind::Function | TokenKind::Class | TokenKind::Abstract | TokenKind::Sealed | TokenKind::Extern | TokenKind::Enum | TokenKind::Interface | TokenKind::Type | TokenKind::Async | TokenKind::Generator) => self.parse_export_decl(),
            TokenKind::Local => self.parse_local(),
            // async function / async generator function
            TokenKind::Async if self.peek_ahead(1).kind == TokenKind::Function
                || (self.peek_ahead(1).kind == TokenKind::Generator && self.peek_ahead(2).kind == TokenKind::Function) => {
                self.advance(); // consume 'async'
                let is_generator = self.check(TokenKind::Generator);
                if is_generator { self.advance(); } // consume 'generator'
                let mut stmt = self.parse_function_decl(false)?;
                if let Statement::FunctionDecl(ref mut fd) = stmt {
                    fd.is_async = true;
                    fd.is_generator = is_generator;
                }
                Ok(stmt)
            }
            // generator function
            TokenKind::Generator if self.peek_ahead(1).kind == TokenKind::Function => {
                self.advance(); // consume 'generator'
                let mut stmt = self.parse_function_decl(false)?;
                if let Statement::FunctionDecl(ref mut fd) = stmt {
                    fd.is_generator = true;
                }
                Ok(stmt)
            }
            TokenKind::Function => self.parse_function_decl(false),
            TokenKind::If => self.parse_if_statement(),
            TokenKind::While => self.parse_while_statement(),
            TokenKind::Repeat => self.parse_repeat_statement(),
            TokenKind::For => self.parse_for_statement(),
            TokenKind::Switch => self.parse_switch_statement(),
            TokenKind::Do => self.parse_do_block(),
            TokenKind::Return => self.parse_return_statement(),
            TokenKind::Break => {
                let span = self.current_span();
                self.advance();
                Ok(Statement::Break(span))
            }
            TokenKind::Continue => {
                let span = self.current_span();
                self.advance();
                Ok(Statement::Continue(span))
            }
            TokenKind::Type if self.peek_ahead(1).kind == TokenKind::Identifier => self.parse_type_alias(),
            _ => self.parse_expr_or_assignment(),
        }
    }

    fn parse_class_decl(
        &mut self,
        is_abstract: bool,
        is_sealed: bool,
        is_extern: bool,
    ) -> ParseResult<Statement> {
        let start = self.current_span();
        self.expect(TokenKind::Class)?;
        let name = self.expect_identifier()?;

        let type_params = if self.check(TokenKind::LessThan) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };

        let parent = if self.check(TokenKind::Extends) {
            self.advance();
            Some(self.parse_type_reference()?)
        } else {
            None
        };

        let interfaces = if self.check(TokenKind::Implements) {
            self.advance();
            let mut ifaces = vec![self.parse_type_reference()?];
            while self.check(TokenKind::Comma) {
                self.advance();
                ifaces.push(self.parse_type_reference()?);
            }
            ifaces
        } else {
            Vec::new()
        };

        let mut members = Vec::new();
        while !self.check(TokenKind::End) && !self.is_at_end() {
            self.skip_semicolons();
            if self.check(TokenKind::End) {
                break;
            }
            members.push(self.parse_class_member()?);
        }

        self.expect(TokenKind::End)?;
        let end = self.previous_span();

        Ok(Statement::ClassDecl(ClassDecl {
            name,
            type_params,
            parent,
            interfaces,
            is_abstract,
            is_sealed,
            is_extern,
            members,
            span: start.merge(end),
        }))
    }

    fn parse_class_member(&mut self) -> ParseResult<ClassMember> {
        let mut access = AccessModifier::Public;
        let mut is_static = false;
        let mut is_abstract = false;
        let mut is_override = false;
        let mut is_readonly = false;
        let mut is_extern = false;
        let mut is_async = false;
        let mut is_generator = false;

        loop {
            match self.current().kind {
                TokenKind::Public => {
                    access = AccessModifier::Public;
                    self.advance();
                }
                TokenKind::Private => {
                    access = AccessModifier::Private;
                    self.advance();
                }
                TokenKind::Protected => {
                    access = AccessModifier::Protected;
                    self.advance();
                }
                TokenKind::Static => {
                    is_static = true;
                    self.advance();
                }
                TokenKind::Abstract => {
                    is_abstract = true;
                    self.advance();
                }
                TokenKind::Override => {
                    is_override = true;
                    self.advance();
                }
                TokenKind::Readonly => {
                    is_readonly = true;
                    self.advance();
                }
                TokenKind::Extern => {
                    is_extern = true;
                    self.advance();
                }
                TokenKind::Async => {
                    is_async = true;
                    self.advance();
                }
                TokenKind::Generator => {
                    is_generator = true;
                    self.advance();
                }
                _ => break,
            }
        }

        if self.check(TokenKind::New) {
            return self.parse_constructor(access);
        }

        if self.check(TokenKind::Get) {
            let next = self.peek_ahead(1);
            if next.kind == TokenKind::Identifier {
                return self.parse_property(access, is_extern);
            }
        }

        if self.check(TokenKind::Set) {
            let next = self.peek_ahead(1);
            if next.kind == TokenKind::Identifier {
                return self.parse_property_setter_only(access, is_extern);
            }
        }

        if self.check(TokenKind::Function) {
            let mut member = self.parse_method(access, is_static, is_abstract, is_override, is_extern)?;
            if let ClassMember::Method(ref mut m) = member {
                m.is_async = is_async;
                m.is_generator = is_generator;
            }
            return Ok(member);
        }

        self.parse_field(access, is_static, is_readonly, is_extern)
    }

    fn parse_constructor(&mut self, access: AccessModifier) -> ParseResult<ClassMember> {
        let start = self.current_span();
        self.expect(TokenKind::New)?;
        self.expect(TokenKind::LeftParen)?;
        let params = self.parse_param_list()?;
        self.expect(TokenKind::RightParen)?;
        let body = self.parse_block()?;
        self.expect(TokenKind::End)?;
        let end = self.previous_span();

        Ok(ClassMember::Constructor(ConstructorDecl {
            params,
            body,
            access,
            span: start.merge(end),
        }))
    }

    fn parse_method(
        &mut self,
        access: AccessModifier,
        is_static: bool,
        is_abstract: bool,
        is_override: bool,
        is_extern: bool,
    ) -> ParseResult<ClassMember> {
        let start = self.current_span();
        self.expect(TokenKind::Function)?;
        let name = self.expect_identifier()?;

        let type_params = if self.check(TokenKind::LessThan) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };

        self.expect(TokenKind::LeftParen)?;
        let params = self.parse_param_list()?;
        self.expect(TokenKind::RightParen)?;

        let return_type = if self.check(TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_annotation()?)
        } else {
            None
        };

        let body = if is_abstract {
            None
        } else {
            let b = self.parse_block()?;
            self.expect(TokenKind::End)?;
            Some(b)
        };

        let end = self.previous_span();

        Ok(ClassMember::Method(MethodDecl {
            name,
            type_params,
            params,
            return_type,
            body,
            access,
            is_static,
            is_abstract,
            is_override,
            is_extern,
            is_async: false,
            is_generator: false,
            span: start.merge(end),
        }))
    }

    fn parse_field(
        &mut self,
        access: AccessModifier,
        is_static: bool,
        is_readonly: bool,
        is_extern: bool,
    ) -> ParseResult<ClassMember> {
        let start = self.current_span();
        let name = self.expect_identifier()?;

        let type_annotation = if self.check(TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_annotation()?)
        } else {
            None
        };

        let default_value = if self.check(TokenKind::Assign) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };

        let end = self.previous_span();

        Ok(ClassMember::Field(FieldDecl {
            name,
            type_annotation,
            default_value,
            access,
            is_static,
            is_readonly,
            is_extern,
            span: start.merge(end),
        }))
    }

    fn parse_property(&mut self, access: AccessModifier, is_extern: bool) -> ParseResult<ClassMember> {
        let start = self.current_span();
        self.expect(TokenKind::Get)?;
        let name = self.expect_identifier()?;

        let type_annotation = if self.check(TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_annotation()?)
        } else {
            None
        };

        let getter_body = self.parse_block()?;
        self.expect(TokenKind::End)?;

        let setter = if self.check(TokenKind::Public)
            || self.check(TokenKind::Private)
            || self.check(TokenKind::Protected)
            || self.check(TokenKind::Set)
        {
            let mut found_set = false;
            if self.check(TokenKind::Set) {
                found_set = true;
            } else {
                self.advance();
                if self.check(TokenKind::Set) {
                    found_set = true;
                }
            }

            if found_set {
                self.advance();
                let set_name_token = self.expect_identifier()?;
                if set_name_token.name != name.name {
                    return Err(self.error(
                        "setter name must match property name",
                        ParseErrorKind::InvalidClassMember,
                    ));
                }
                self.expect(TokenKind::LeftParen)?;
                let param = self.expect_identifier()?;
                if self.check(TokenKind::Colon) {
                    self.advance();
                    let _ = self.parse_type_annotation()?;
                }
                self.expect(TokenKind::RightParen)?;
                let setter_body = self.parse_block()?;
                self.expect(TokenKind::End)?;
                Some((param, setter_body))
            } else {
                None
            }
        } else {
            None
        };

        let end = self.previous_span();

        Ok(ClassMember::Property(PropertyDecl {
            name,
            type_annotation,
            getter: Some(getter_body),
            setter,
            access,
            is_extern,
            span: start.merge(end),
        }))
    }

    fn parse_property_setter_only(&mut self, access: AccessModifier, is_extern: bool) -> ParseResult<ClassMember> {
        let start = self.current_span();
        self.expect(TokenKind::Set)?;
        let name = self.expect_identifier()?;
        self.expect(TokenKind::LeftParen)?;
        let param = self.expect_identifier()?;
        if self.check(TokenKind::Colon) {
            self.advance();
            let _ = self.parse_type_annotation()?;
        }
        self.expect(TokenKind::RightParen)?;
        let body = self.parse_block()?;
        self.expect(TokenKind::End)?;
        let end = self.previous_span();

        Ok(ClassMember::Property(PropertyDecl {
            name,
            type_annotation: None,
            getter: None,
            setter: Some((param, body)),
            access,
            is_extern,
            span: start.merge(end),
        }))
    }

    fn parse_import_decl(&mut self) -> ParseResult<Statement> {
        let start = self.current_span();
        self.expect(TokenKind::Import)?;

        let mut names = Vec::new();
        loop {
            let name_start = self.current_span();
            let name = self.expect_identifier()?;
            let alias = if self.check(TokenKind::As) {
                self.advance();
                Some(self.expect_identifier()?)
            } else {
                None
            };
            let name_end = self.previous_span();
            names.push(ImportName {
                name,
                alias,
                span: name_start.merge(name_end),
            });
            if !self.check(TokenKind::Comma) {
                break;
            }
            self.advance();
        }

        self.expect(TokenKind::From)?;

        let path = if self.check(TokenKind::StringLiteral) {
            let s = self.current().lexeme.clone();
            self.advance();
            // Strip quotes
            let inner = &s[1..s.len() - 1];
            smol_str::SmolStr::new(inner)
        } else {
            return Err(self.error(
                "expected string path after 'from'",
                ParseErrorKind::InvalidStatement,
            ));
        };

        let end = self.previous_span();
        Ok(Statement::ImportDecl(ImportDecl {
            names,
            path,
            span: start.merge(end),
        }))
    }

    fn parse_export_decl(&mut self) -> ParseResult<Statement> {
        let start = self.current_span();
        self.expect(TokenKind::Export)?;

        // Parse the inner statement (local, function, class, enum, type)
        let inner = match self.current().kind {
            TokenKind::Local => self.parse_local()?,
            TokenKind::Function => self.parse_function_decl(false)?,
            TokenKind::Class => self.parse_class_decl(false, false, false)?,
            TokenKind::Abstract => {
                self.advance();
                if self.check(TokenKind::Class) {
                    self.parse_class_decl(true, false, false)?
                } else {
                    return Err(self.error_at(
                        start,
                        "expected 'class' after 'abstract'",
                        ParseErrorKind::InvalidStatement,
                    ));
                }
            }
            TokenKind::Sealed => {
                self.advance();
                if self.check(TokenKind::Class) {
                    self.parse_class_decl(false, true, false)?
                } else {
                    return Err(self.error_at(
                        start,
                        "expected 'class' after 'sealed'",
                        ParseErrorKind::InvalidStatement,
                    ));
                }
            }
            TokenKind::Extern => {
                self.advance();
                match self.current().kind {
                    TokenKind::Class => self.parse_class_decl(false, false, true)?,
                    TokenKind::Abstract => {
                        self.advance();
                        if self.check(TokenKind::Class) {
                            self.parse_class_decl(true, false, true)?
                        } else {
                            return Err(self.error_at(
                                start,
                                "expected 'class' after 'extern abstract'",
                                ParseErrorKind::InvalidStatement,
                            ));
                        }
                    }
                    TokenKind::Sealed => {
                        self.advance();
                        if self.check(TokenKind::Class) {
                            self.parse_class_decl(false, true, true)?
                        } else {
                            return Err(self.error_at(
                                start,
                                "expected 'class' after 'extern sealed'",
                                ParseErrorKind::InvalidStatement,
                            ));
                        }
                    }
                    TokenKind::Interface => self.parse_interface_decl_with_extern(true)?,
                    TokenKind::Enum => self.parse_enum_decl_with_extern(true)?,
                    _ => {
                        return Err(self.error_at(
                            start,
                            "expected 'class', 'interface', or 'enum' after 'extern'",
                            ParseErrorKind::InvalidStatement,
                        ));
                    }
                }
            }
            TokenKind::Enum => self.parse_enum_decl_with_extern(false)?,
            TokenKind::Interface => self.parse_interface_decl_with_extern(false)?,
            TokenKind::Type => self.parse_type_alias()?,
            TokenKind::Async if self.peek_ahead(1).kind == TokenKind::Function
                || (self.peek_ahead(1).kind == TokenKind::Generator && self.peek_ahead(2).kind == TokenKind::Function) => {
                self.advance(); // consume 'async'
                let is_generator = self.check(TokenKind::Generator);
                if is_generator { self.advance(); }
                let mut stmt = self.parse_function_decl(false)?;
                if let Statement::FunctionDecl(ref mut fd) = stmt {
                    fd.is_async = true;
                    fd.is_generator = is_generator;
                }
                stmt
            }
            TokenKind::Generator if self.peek_ahead(1).kind == TokenKind::Function => {
                self.advance(); // consume 'generator'
                let mut stmt = self.parse_function_decl(false)?;
                if let Statement::FunctionDecl(ref mut fd) = stmt {
                    fd.is_generator = true;
                }
                stmt
            }
            _ => {
                return Err(self.error(
                    "expected declaration after 'export'",
                    ParseErrorKind::InvalidStatement,
                ));
            }
        };

        let end = self.previous_span();
        Ok(Statement::ExportDecl(Box::new(inner), start.merge(end)))
    }

    fn parse_type_alias(&mut self) -> ParseResult<Statement> {
        let start = self.current_span();
        self.expect(TokenKind::Type)?;
        let name = self.expect_identifier()?;

        let type_params = if self.check(TokenKind::LessThan) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };

        self.expect(TokenKind::Assign)?;
        let value = self.parse_type_annotation()?;
        let end = self.previous_span();

        Ok(Statement::TypeAlias(TypeAliasDecl {
            name,
            type_params,
            value,
            span: start.merge(end),
        }))
    }

    fn parse_interface_decl_with_extern(&mut self, is_extern: bool) -> ParseResult<Statement> {
        let start = self.current_span();
        self.expect(TokenKind::Interface)?;
        let name = self.expect_identifier()?;

        let type_params = if self.check(TokenKind::LessThan) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };

        let extends = if self.check(TokenKind::Extends) {
            self.advance();
            let mut parents = vec![self.parse_type_reference()?];
            while self.check(TokenKind::Comma) {
                self.advance();
                parents.push(self.parse_type_reference()?);
            }
            parents
        } else {
            Vec::new()
        };

        let mut members = Vec::new();
        while !self.check(TokenKind::End) && !self.is_at_end() {
            self.skip_semicolons();
            if self.check(TokenKind::End) {
                break;
            }
            members.push(self.parse_interface_member()?);
        }

        self.expect(TokenKind::End)?;
        let end = self.previous_span();

        Ok(Statement::InterfaceDecl(InterfaceDecl {
            name,
            type_params,
            extends,
            members,
            is_extern,
            span: start.merge(end),
        }))
    }

    fn parse_interface_member(&mut self) -> ParseResult<InterfaceMember> {
        let is_extern = if self.check(TokenKind::Extern) {
            self.advance();
            true
        } else {
            false
        };

        // If next is `function`, it's a method
        if self.check(TokenKind::Function) {
            return Ok(InterfaceMember::Method(self.parse_interface_method_inner(is_extern)?));
        }

        // Otherwise it's a field: name: type
        let start = self.current_span();
        let name = self.expect_identifier()?;
        self.expect(TokenKind::Colon)?;
        let type_annotation = self.parse_type_annotation()?;
        let end = self.previous_span();

        Ok(InterfaceMember::Field(InterfaceField {
            name,
            type_annotation,
            is_extern,
            span: start.merge(end),
        }))
    }

    fn parse_interface_method_inner(&mut self, is_extern: bool) -> ParseResult<InterfaceMethod> {
        let start = self.current_span();
        self.expect(TokenKind::Function)?;
        let name = self.expect_identifier()?;

        let type_params = if self.check(TokenKind::LessThan) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };

        self.expect(TokenKind::LeftParen)?;
        let params = self.parse_param_list()?;
        self.expect(TokenKind::RightParen)?;

        let return_type = if self.check(TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_annotation()?)
        } else {
            None
        };

        let end = self.previous_span();

        Ok(InterfaceMethod {
            name,
            type_params,
            params,
            return_type,
            is_extern,
            span: start.merge(end),
        })
    }

    fn parse_enum_decl_with_extern(&mut self, is_type_extern: bool) -> ParseResult<Statement> {
        let start = self.current_span();
        self.expect(TokenKind::Enum)?;
        let name = self.expect_identifier()?;

        let mut variants = Vec::new();
        while !self.check(TokenKind::End) && !self.is_at_end() {
            self.skip_semicolons();
            if self.check(TokenKind::End) {
                break;
            }
            let var_start = self.current_span();

            let is_extern = if is_type_extern {
                true
            } else if self.check(TokenKind::Extern) {
                self.advance();
                true
            } else {
                false
            };

            let var_name = self.expect_identifier()?;

            let value = if self.check(TokenKind::Assign) {
                self.advance();
                Some(self.parse_expression()?)
            } else {
                None
            };

            let var_end = self.previous_span();
            variants.push(EnumVariant {
                name: var_name,
                value,
                is_extern,
                span: var_start.merge(var_end),
            });
        }

        self.expect(TokenKind::End)?;
        let end = self.previous_span();

        Ok(Statement::EnumDecl(EnumDecl {
            name,
            variants,
            is_extern: is_type_extern,
            span: start.merge(end),
        }))
    }

    fn parse_local(&mut self) -> ParseResult<Statement> {
        let start = self.current_span();
        self.expect(TokenKind::Local)?;

        // local async function / local async generator function
        if self.check(TokenKind::Async) && (self.peek_ahead(1).kind == TokenKind::Function
            || (self.peek_ahead(1).kind == TokenKind::Generator && self.peek_ahead(2).kind == TokenKind::Function)) {
            self.advance(); // consume 'async'
            let is_generator = self.check(TokenKind::Generator);
            if is_generator { self.advance(); }
            let mut stmt = self.parse_function_decl(true)?;
            if let Statement::FunctionDecl(ref mut fd) = stmt {
                fd.is_async = true;
                fd.is_generator = is_generator;
            }
            return Ok(stmt);
        }

        // local generator function
        if self.check(TokenKind::Generator) && self.peek_ahead(1).kind == TokenKind::Function {
            self.advance(); // consume 'generator'
            let mut stmt = self.parse_function_decl(true)?;
            if let Statement::FunctionDecl(ref mut fd) = stmt {
                fd.is_generator = true;
            }
            return Ok(stmt);
        }

        if self.check(TokenKind::Function) {
            return self.parse_function_decl(true);
        }

        let mut names = vec![self.expect_identifier()?];
        let mut type_annotations = Vec::new();

        let ta = if self.check(TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_annotation()?)
        } else {
            None
        };
        type_annotations.push(ta);

        while self.check(TokenKind::Comma) {
            self.advance();
            names.push(self.expect_identifier()?);
            let ta = if self.check(TokenKind::Colon) {
                self.advance();
                Some(self.parse_type_annotation()?)
            } else {
                None
            };
            type_annotations.push(ta);
        }

        let values = if self.check(TokenKind::Assign) {
            self.advance();
            self.parse_expression_list()?
        } else {
            Vec::new()
        };

        let end = self.previous_span();

        Ok(Statement::LocalAssignment(LocalAssignment {
            names,
            type_annotations,
            values,
            span: start.merge(end),
        }))
    }

    fn parse_function_decl(&mut self, is_local: bool) -> ParseResult<Statement> {
        let start = self.current_span();
        self.expect(TokenKind::Function)?;

        let name = self.parse_function_name()?;

        let type_params = if self.check(TokenKind::LessThan) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };

        self.expect(TokenKind::LeftParen)?;
        let params = self.parse_param_list()?;
        self.expect(TokenKind::RightParen)?;

        let return_type = if self.check(TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_annotation()?)
        } else {
            None
        };

        let body = self.parse_block()?;
        self.expect(TokenKind::End)?;
        let end = self.previous_span();

        Ok(Statement::FunctionDecl(FunctionDecl {
            name,
            type_params,
            params,
            return_type,
            body,
            is_local,
            is_async: false,
            is_generator: false,
            span: start.merge(end),
        }))
    }

    fn parse_function_name(&mut self) -> ParseResult<FunctionName> {
        let start = self.current_span();
        let mut parts = vec![self.expect_identifier()?];
        let mut method = None;

        while self.check(TokenKind::Dot) {
            self.advance();
            parts.push(self.expect_identifier()?);
        }

        if self.check(TokenKind::Colon) {
            self.advance();
            method = Some(self.expect_identifier()?);
        }

        let end = self.previous_span();
        Ok(FunctionName {
            parts,
            method,
            span: start.merge(end),
        })
    }

    fn parse_if_statement(&mut self) -> ParseResult<Statement> {
        let start = self.current_span();
        self.expect(TokenKind::If)?;
        let condition = self.parse_expression()?;
        self.expect(TokenKind::Then)?;
        let then_block = self.parse_block()?;

        let mut elseif_clauses = Vec::new();
        while self.check(TokenKind::ElseIf) {
            self.advance();
            let cond = self.parse_expression()?;
            self.expect(TokenKind::Then)?;
            let block = self.parse_block()?;
            elseif_clauses.push((cond, block));
        }

        let else_block = if self.check(TokenKind::Else) {
            self.advance();
            Some(self.parse_block()?)
        } else {
            None
        };

        self.expect(TokenKind::End)?;
        let end = self.previous_span();

        Ok(Statement::IfStatement(IfStatement {
            condition,
            then_block,
            elseif_clauses,
            else_block,
            span: start.merge(end),
        }))
    }

    fn parse_while_statement(&mut self) -> ParseResult<Statement> {
        let start = self.current_span();
        self.expect(TokenKind::While)?;
        let condition = self.parse_expression()?;
        self.expect(TokenKind::Do)?;
        let body = self.parse_block()?;
        self.expect(TokenKind::End)?;
        let end = self.previous_span();

        Ok(Statement::WhileStatement(WhileStatement {
            condition,
            body,
            span: start.merge(end),
        }))
    }

    fn parse_repeat_statement(&mut self) -> ParseResult<Statement> {
        let start = self.current_span();
        self.expect(TokenKind::Repeat)?;
        let body = self.parse_block()?;
        self.expect(TokenKind::Until)?;
        let condition = self.parse_expression()?;
        let end = self.previous_span();

        Ok(Statement::RepeatStatement(RepeatStatement {
            body,
            condition,
            span: start.merge(end),
        }))
    }

    fn parse_for_statement(&mut self) -> ParseResult<Statement> {
        let start = self.current_span();
        self.expect(TokenKind::For)?;
        let name = self.expect_identifier()?;

        if self.check(TokenKind::Assign) {
            self.advance();
            let start_expr = self.parse_expression()?;
            self.expect(TokenKind::Comma)?;
            let stop = self.parse_expression()?;
            let step = if self.check(TokenKind::Comma) {
                self.advance();
                Some(self.parse_expression()?)
            } else {
                None
            };
            self.expect(TokenKind::Do)?;
            let body = self.parse_block()?;
            self.expect(TokenKind::End)?;
            let end = self.previous_span();

            Ok(Statement::ForNumeric(ForNumericStatement {
                name,
                start: start_expr,
                stop,
                step,
                body,
                span: start.merge(end),
            }))
        } else {
            let mut names = vec![name];
            while self.check(TokenKind::Comma) {
                self.advance();
                names.push(self.expect_identifier()?);
            }
            self.expect(TokenKind::In)?;
            let iterators = self.parse_expression_list()?;
            self.expect(TokenKind::Do)?;
            let body = self.parse_block()?;
            self.expect(TokenKind::End)?;
            let end = self.previous_span();

            Ok(Statement::ForGeneric(ForGenericStatement {
                names,
                iterators,
                body,
                span: start.merge(end),
            }))
        }
    }

    fn parse_switch_statement(&mut self) -> ParseResult<Statement> {
        let start = self.current_span();
        self.expect(TokenKind::Switch)?;
        let subject = self.parse_expression()?;
        self.expect(TokenKind::Do)?;

        let mut cases = Vec::new();
        let mut default = None;

        loop {
            if self.check(TokenKind::Case) {
                let case_start = self.current_span();
                self.advance();
                // Parse one or more comma-separated values
                let mut values = vec![self.parse_expression()?];
                while self.check(TokenKind::Comma) {
                    self.advance();
                    values.push(self.parse_expression()?);
                }
                self.expect(TokenKind::Then)?;
                let body = self.parse_block()?;
                let case_end = self.previous_span();
                cases.push(SwitchCase {
                    values,
                    body,
                    span: case_start.merge(case_end),
                });
            } else if self.check(TokenKind::Default) {
                self.advance();
                self.expect(TokenKind::Then)?;
                default = Some(self.parse_block()?);
            } else {
                break;
            }
        }

        self.expect(TokenKind::End)?;
        let end = self.previous_span();

        Ok(Statement::SwitchStatement(SwitchStatement {
            subject,
            cases,
            default,
            span: start.merge(end),
        }))
    }

    fn parse_do_block(&mut self) -> ParseResult<Statement> {
        self.expect(TokenKind::Do)?;
        let block = self.parse_block()?;
        self.expect(TokenKind::End)?;
        Ok(Statement::DoBlock(block))
    }

    fn parse_return_statement(&mut self) -> ParseResult<Statement> {
        let start = self.current_span();
        self.expect(TokenKind::Return)?;

        let values = if self.is_block_end() || self.check(TokenKind::Semicolon) {
            Vec::new()
        } else {
            self.parse_expression_list()?
        };

        self.skip_semicolons();
        let end = self.previous_span();

        Ok(Statement::ReturnStatement(ReturnStatement {
            values,
            span: start.merge(end),
        }))
    }

    fn parse_expr_or_assignment(&mut self) -> ParseResult<Statement> {
        let expr = self.parse_suffixed_expression()?;

        // Check for compound assignment (+=, -=, etc.)
        let compound_op = match self.current().kind {
            TokenKind::PlusAssign => Some(CompoundOp::Add),
            TokenKind::MinusAssign => Some(CompoundOp::Sub),
            TokenKind::StarAssign => Some(CompoundOp::Mul),
            TokenKind::SlashAssign => Some(CompoundOp::Div),
            TokenKind::PercentAssign => Some(CompoundOp::Mod),
            TokenKind::CaretAssign => Some(CompoundOp::Pow),
            TokenKind::DotDotAssign => Some(CompoundOp::Concat),
            _ => None,
        };

        if let Some(op) = compound_op {
            let start = expr.span();
            self.advance();
            let value = self.parse_expression()?;
            let end = self.previous_span();
            return Ok(Statement::CompoundAssignment(CompoundAssignment {
                target: expr,
                op,
                value,
                span: start.merge(end),
            }));
        }

        if self.check(TokenKind::Assign) {
            let start = expr.span();
            let mut targets = vec![expr];

            while self.check(TokenKind::Comma) {
                self.advance();
                targets.push(self.parse_suffixed_expression()?);
                if !self.check(TokenKind::Comma) && !self.check(TokenKind::Assign) {
                    break;
                }
            }

            self.expect(TokenKind::Assign)?;
            let values = self.parse_expression_list()?;
            let end = self.previous_span();

            Ok(Statement::Assignment(Assignment {
                targets,
                values,
                span: start.merge(end),
            }))
        } else {
            Ok(Statement::ExpressionStatement(expr))
        }
    }

    fn parse_block(&mut self) -> ParseResult<Block> {
        let start = self.current_span();
        let mut statements = Vec::new();

        while !self.is_block_end() && !self.is_at_end() {
            match self.parse_statement() {
                Ok(stmt) => statements.push(stmt),
                Err(e) => {
                    self.errors.push(e);
                    self.synchronize();
                }
            }
        }

        let end = self.current_span();
        Ok(Block {
            statements,
            span: start.merge(end),
        })
    }

    fn is_block_end(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::End
                | TokenKind::Else
                | TokenKind::ElseIf
                | TokenKind::Until
                | TokenKind::Case
                | TokenKind::Default
                | TokenKind::Eof
        )
    }

    fn parse_expression(&mut self) -> ParseResult<Expression> {
        self.parse_or_expr()
    }

    fn parse_or_expr(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_and_expr()?;

        while self.check(TokenKind::Or) {
            self.advance();
            let right = self.parse_and_expr()?;
            let span = left.span().merge(right.span());
            left = Expression::BinaryOp(Box::new(BinaryOp {
                left,
                op: BinOp::Or,
                right,
                span,
            }));
        }

        Ok(left)
    }

    fn parse_and_expr(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_comparison_expr()?;

        while self.check(TokenKind::And) {
            self.advance();
            let right = self.parse_comparison_expr()?;
            let span = left.span().merge(right.span());
            left = Expression::BinaryOp(Box::new(BinaryOp {
                left,
                op: BinOp::And,
                right,
                span,
            }));
        }

        Ok(left)
    }

    fn parse_comparison_expr(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_bitor_expr()?;

        loop {
            let op = match self.current().kind {
                TokenKind::LessThan => BinOp::Lt,
                TokenKind::GreaterThan => BinOp::Gt,
                TokenKind::LessEqual => BinOp::Le,
                TokenKind::GreaterEqual => BinOp::Ge,
                TokenKind::Equal => BinOp::Eq,
                TokenKind::NotEqual => BinOp::NotEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_bitor_expr()?;
            let span = left.span().merge(right.span());
            left = Expression::BinaryOp(Box::new(BinaryOp {
                left,
                op,
                right,
                span,
            }));
        }

        if self.check(TokenKind::Instanceof) {
            self.advance();
            let class_name = self.expect_identifier()?;
            let span = left.span().merge(class_name.span);
            left = Expression::Instanceof(Box::new(InstanceofExpr {
                object: left,
                class_name,
                span,
            }));
        }

        if self.check(TokenKind::As) {
            self.advance();
            let target_type = self.parse_type_annotation()?;
            let span = left.span().merge(target_type.span);
            left = Expression::CastExpr(Box::new(CastExpr {
                expr: left,
                target_type,
                span,
            }));
        }

        Ok(left)
    }

    fn parse_bitor_expr(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_bitxor_expr()?;

        while self.check(TokenKind::Pipe) {
            self.advance();
            let right = self.parse_bitxor_expr()?;
            let span = left.span().merge(right.span());
            left = Expression::BinaryOp(Box::new(BinaryOp {
                left,
                op: BinOp::BitOr,
                right,
                span,
            }));
        }

        Ok(left)
    }

    fn parse_bitxor_expr(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_bitand_expr()?;

        while self.check(TokenKind::Tilde) {
            self.advance();
            let right = self.parse_bitand_expr()?;
            let span = left.span().merge(right.span());
            left = Expression::BinaryOp(Box::new(BinaryOp {
                left,
                op: BinOp::BitXor,
                right,
                span,
            }));
        }

        Ok(left)
    }

    fn parse_bitand_expr(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_shift_expr()?;

        while self.check(TokenKind::Ampersand) {
            self.advance();
            let right = self.parse_shift_expr()?;
            let span = left.span().merge(right.span());
            left = Expression::BinaryOp(Box::new(BinaryOp {
                left,
                op: BinOp::BitAnd,
                right,
                span,
            }));
        }

        Ok(left)
    }

    fn parse_shift_expr(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_concat_expr()?;

        loop {
            let op = match self.current().kind {
                TokenKind::ShiftLeft => BinOp::ShiftLeft,
                TokenKind::ShiftRight => BinOp::ShiftRight,
                _ => break,
            };
            self.advance();
            let right = self.parse_concat_expr()?;
            let span = left.span().merge(right.span());
            left = Expression::BinaryOp(Box::new(BinaryOp {
                left,
                op,
                right,
                span,
            }));
        }

        Ok(left)
    }

    fn parse_concat_expr(&mut self) -> ParseResult<Expression> {
        let left = self.parse_additive_expr()?;

        if self.check(TokenKind::DotDot) {
            self.advance();
            let right = self.parse_concat_expr()?;
            let span = left.span().merge(right.span());
            Ok(Expression::BinaryOp(Box::new(BinaryOp {
                left,
                op: BinOp::Concat,
                right,
                span,
            })))
        } else {
            Ok(left)
        }
    }

    fn parse_additive_expr(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_multiplicative_expr()?;

        loop {
            let op = match self.current().kind {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative_expr()?;
            let span = left.span().merge(right.span());
            left = Expression::BinaryOp(Box::new(BinaryOp {
                left,
                op,
                right,
                span,
            }));
        }

        Ok(left)
    }

    fn parse_multiplicative_expr(&mut self) -> ParseResult<Expression> {
        let mut left = self.parse_unary_expr()?;

        loop {
            let op = match self.current().kind {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::DoubleSlash => BinOp::IntDiv,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary_expr()?;
            let span = left.span().merge(right.span());
            left = Expression::BinaryOp(Box::new(BinaryOp {
                left,
                op,
                right,
                span,
            }));
        }

        Ok(left)
    }

    fn parse_unary_expr(&mut self) -> ParseResult<Expression> {
        let op = match self.current().kind {
            TokenKind::Not => Some(UnOp::Not),
            TokenKind::Hash => Some(UnOp::Len),
            TokenKind::Minus => Some(UnOp::Neg),
            TokenKind::Tilde => Some(UnOp::BitNot),
            _ => None,
        };

        if let Some(op) = op {
            let start = self.current_span();
            self.advance();
            let operand = self.parse_unary_expr()?;
            let span = start.merge(operand.span());
            Ok(Expression::UnaryOp(Box::new(UnaryOp {
                op,
                operand,
                span,
            })))
        } else {
            self.parse_power_expr()
        }
    }

    fn parse_power_expr(&mut self) -> ParseResult<Expression> {
        let base = self.parse_suffixed_expression()?;

        if self.check(TokenKind::Caret) {
            self.advance();
            let exp = self.parse_unary_expr()?;
            let span = base.span().merge(exp.span());
            Ok(Expression::BinaryOp(Box::new(BinaryOp {
                left: base,
                op: BinOp::Pow,
                right: exp,
                span,
            })))
        } else {
            Ok(base)
        }
    }

    fn parse_suffixed_expression(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_primary_expression()?;

        loop {
            match self.current().kind {
                TokenKind::Dot => {
                    self.advance();
                    let field = self.expect_identifier()?;
                    let span = expr.span().merge(field.span);
                    expr = Expression::FieldAccess(Box::new(FieldAccess {
                        object: expr,
                        field,
                        span,
                    }));
                }
                TokenKind::LeftBracket => {
                    self.advance();
                    let index = self.parse_expression()?;
                    self.expect(TokenKind::RightBracket)?;
                    let span = expr.span().merge(self.previous_span());
                    expr = Expression::IndexAccess(Box::new(IndexAccess {
                        object: expr,
                        index,
                        span,
                    }));
                }
                TokenKind::Colon => {
                    self.advance();
                    let method = self.expect_identifier()?;
                    self.expect(TokenKind::LeftParen)?;
                    let args = if self.check(TokenKind::RightParen) {
                        Vec::new()
                    } else {
                        self.parse_expression_list()?
                    };
                    self.expect(TokenKind::RightParen)?;
                    let span = expr.span().merge(self.previous_span());
                    expr = Expression::MethodCall(Box::new(MethodCall {
                        object: expr,
                        method,
                        args,
                        span,
                    }));
                }
                TokenKind::LeftParen => {
                    self.advance();
                    let args = if self.check(TokenKind::RightParen) {
                        Vec::new()
                    } else {
                        self.parse_expression_list()?
                    };
                    self.expect(TokenKind::RightParen)?;
                    let span = expr.span().merge(self.previous_span());
                    expr = Expression::FunctionCall(Box::new(FunctionCall {
                        callee: expr,
                        args,
                        span,
                    }));
                }
                TokenKind::LeftBrace => {
                    let table = self.parse_table_constructor()?;
                    let span = expr.span().merge(table.span);
                    expr = Expression::FunctionCall(Box::new(FunctionCall {
                        callee: expr,
                        args: vec![Expression::TableConstructor(Box::new(table))],
                        span,
                    }));
                }
                TokenKind::StringLiteral => {
                    let s = self.current().clone();
                    self.advance();
                    let str_span = s.span;
                    let span = expr.span().merge(str_span);
                    expr = Expression::FunctionCall(Box::new(FunctionCall {
                        callee: expr,
                        args: vec![Expression::String(s.lexeme, str_span)],
                        span,
                    }));
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_primary_expression(&mut self) -> ParseResult<Expression> {
        match self.current().kind {
            TokenKind::Nil => {
                let span = self.current_span();
                self.advance();
                Ok(Expression::Nil(span))
            }
            TokenKind::True => {
                let span = self.current_span();
                self.advance();
                Ok(Expression::True(span))
            }
            TokenKind::False => {
                let span = self.current_span();
                self.advance();
                Ok(Expression::False(span))
            }
            TokenKind::Number => {
                let token = self.current().clone();
                self.advance();
                Ok(Expression::Number(token.lexeme, token.span))
            }
            TokenKind::StringLiteral => {
                let token = self.current().clone();
                self.advance();
                Ok(Expression::String(token.lexeme, token.span))
            }
            TokenKind::DotDotDot => {
                let span = self.current_span();
                self.advance();
                Ok(Expression::Vararg(span))
            }
            TokenKind::Identifier => {
                let id = self.expect_identifier()?;
                Ok(Expression::Identifier(id))
            }
            TokenKind::LeftParen => {
                let start = self.current_span();
                self.advance();
                // Empty tuple: ()
                if self.check(TokenKind::RightParen) {
                    self.advance();
                    let end = self.previous_span();
                    return Ok(Expression::TupleLiteral(Box::new(TupleLiteral {
                        elements: Vec::new(),
                        span: start.merge(end),
                    })));
                }
                let expr = self.parse_expression()?;
                if self.check(TokenKind::Comma) {
                    // Tuple: (expr, ...) or (expr,)
                    let mut elements = vec![expr];
                    while self.check(TokenKind::Comma) {
                        self.advance();
                        if self.check(TokenKind::RightParen) {
                            break; // trailing comma
                        }
                        elements.push(self.parse_expression()?);
                    }
                    self.expect(TokenKind::RightParen)?;
                    let end = self.previous_span();
                    Ok(Expression::TupleLiteral(Box::new(TupleLiteral {
                        elements,
                        span: start.merge(end),
                    })))
                } else {
                    // Grouped expression: (expr)
                    self.expect(TokenKind::RightParen)?;
                    let end = self.previous_span();
                    let span = Span { start: start.start, end: end.end };
                    Ok(Expression::Grouped(Box::new(expr), span))
                }
            }
            TokenKind::Function => {
                let start = self.current_span();
                self.advance();
                self.expect(TokenKind::LeftParen)?;
                let params = self.parse_param_list()?;
                self.expect(TokenKind::RightParen)?;

                let return_type = if self.check(TokenKind::Colon) {
                    self.advance();
                    Some(self.parse_type_annotation()?)
                } else {
                    None
                };

                let body = self.parse_block()?;
                self.expect(TokenKind::End)?;
                let end = self.previous_span();

                Ok(Expression::FunctionExpr(Box::new(FunctionExpr {
                    params,
                    return_type,
                    body,
                    is_async: false,
                    is_generator: false,
                    span: start.merge(end),
                })))
            }
            TokenKind::LeftBracket => {
                let start = self.current_span();
                self.advance(); // consume '['
                let mut elements = Vec::new();
                if !self.check(TokenKind::RightBracket) {
                    elements.push(self.parse_expression()?);
                    while self.check(TokenKind::Comma) {
                        self.advance();
                        if self.check(TokenKind::RightBracket) {
                            break; // trailing comma
                        }
                        elements.push(self.parse_expression()?);
                    }
                }
                self.expect(TokenKind::RightBracket)?;
                let end = self.previous_span();
                Ok(Expression::ArrayLiteral(Box::new(ArrayLiteral {
                    elements,
                    span: start.merge(end),
                })))
            }
            TokenKind::LeftBrace => {
                let table = self.parse_table_constructor()?;
                Ok(Expression::TableConstructor(Box::new(table)))
            }
            TokenKind::Super => {
                let start = self.current_span();
                self.advance();
                self.expect(TokenKind::Dot)?;
                let method = self.expect_identifier_or_keyword()?;
                let span = start.merge(method.span);
                Ok(Expression::SuperAccess(Box::new(SuperAccess {
                    method,
                    span,
                })))
            }
            TokenKind::New => {
                let start = self.current_span();
                self.advance();
                let class_name = self.parse_type_reference()?;
                self.expect(TokenKind::LeftParen)?;
                let args = if self.check(TokenKind::RightParen) {
                    Vec::new()
                } else {
                    self.parse_expression_list()?
                };
                self.expect(TokenKind::RightParen)?;
                let end = self.previous_span();

                Ok(Expression::NewExpr(Box::new(NewExpr {
                    class_name,
                    args,
                    span: start.merge(end),
                })))
            }
            // Luau if-expression: if cond then expr [elseif cond then expr]* else expr
            TokenKind::If => {
                let start = self.current_span();
                self.advance();
                let condition = self.parse_expression()?;
                self.expect(TokenKind::Then)?;
                let then_expr = self.parse_expression()?;

                let mut elseif_clauses = Vec::new();
                while self.check(TokenKind::ElseIf) {
                    self.advance();
                    let eif_cond = self.parse_expression()?;
                    self.expect(TokenKind::Then)?;
                    let eif_expr = self.parse_expression()?;
                    elseif_clauses.push((eif_cond, eif_expr));
                }

                self.expect(TokenKind::Else)?;
                let else_expr = self.parse_expression()?;
                let end = self.previous_span();

                Ok(Expression::IfExpression(Box::new(IfExpr {
                    condition,
                    then_expr,
                    elseif_clauses,
                    else_expr,
                    span: start.merge(end),
                })))
            }
            TokenKind::Yield => {
                let start = self.current_span();
                self.advance();
                // yield can have an optional value — check if next token can start an expression
                let value = if self.can_start_expression() {
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                let end = self.previous_span();
                Ok(Expression::YieldExpr(Box::new(YieldExpr {
                    value,
                    span: start.merge(end),
                })))
            }
            TokenKind::Await => {
                let start = self.current_span();
                self.advance();
                let expr = self.parse_expression()?;
                let end = self.previous_span();
                Ok(Expression::AwaitExpr(Box::new(AwaitExpr {
                    expr,
                    span: start.merge(end),
                })))
            }
            TokenKind::Async if self.peek_ahead(1).kind == TokenKind::Function => {
                let start = self.current_span();
                self.advance(); // consume 'async'
                let is_gen = self.check(TokenKind::Generator);
                if is_gen { self.advance(); }
                self.advance(); // consume 'function'
                self.expect(TokenKind::LeftParen)?;
                let params = self.parse_param_list()?;
                self.expect(TokenKind::RightParen)?;
                let return_type = if self.check(TokenKind::Colon) {
                    self.advance();
                    Some(self.parse_type_annotation()?)
                } else {
                    None
                };
                let body = self.parse_block()?;
                self.expect(TokenKind::End)?;
                let end = self.previous_span();
                Ok(Expression::FunctionExpr(Box::new(FunctionExpr {
                    params,
                    return_type,
                    body,
                    is_async: true,
                    is_generator: is_gen,
                    span: start.merge(end),
                })))
            }
            TokenKind::Generator if self.peek_ahead(1).kind == TokenKind::Function => {
                let start = self.current_span();
                self.advance(); // consume 'generator'
                self.advance(); // consume 'function'
                self.expect(TokenKind::LeftParen)?;
                let params = self.parse_param_list()?;
                self.expect(TokenKind::RightParen)?;
                let return_type = if self.check(TokenKind::Colon) {
                    self.advance();
                    Some(self.parse_type_annotation()?)
                } else {
                    None
                };
                let body = self.parse_block()?;
                self.expect(TokenKind::End)?;
                let end = self.previous_span();
                Ok(Expression::FunctionExpr(Box::new(FunctionExpr {
                    params,
                    return_type,
                    body,
                    is_async: false,
                    is_generator: true,
                    span: start.merge(end),
                })))
            }
            _ if self.current().kind.is_contextual_keyword() => {
                let token = self.current().clone();
                self.advance();
                Ok(Expression::Identifier(Identifier {
                    name: token.lexeme,
                    span: token.span,
                }))
            }
            _ => Err(self.error(
                &format!("unexpected token '{}'", self.current().kind),
                ParseErrorKind::InvalidExpression,
            )),
        }
    }

    fn parse_table_constructor(&mut self) -> ParseResult<TableConstructor> {
        let start = self.current_span();
        self.expect(TokenKind::LeftBrace)?;

        let mut fields = Vec::new();
        while !self.check(TokenKind::RightBrace) && !self.is_at_end() {
            let field = self.parse_table_field()?;
            fields.push(field);

            if !self.check(TokenKind::Comma) && !self.check(TokenKind::Semicolon) {
                break;
            }
            self.advance();
        }

        self.expect(TokenKind::RightBrace)?;
        let end = self.previous_span();

        Ok(TableConstructor {
            fields,
            span: start.merge(end),
        })
    }

    fn parse_table_field(&mut self) -> ParseResult<TableField> {
        let start = self.current_span();

        if self.check(TokenKind::LeftBracket) {
            self.advance();
            let key = self.parse_expression()?;
            self.expect(TokenKind::RightBracket)?;
            self.expect(TokenKind::Assign)?;
            let value = self.parse_expression()?;
            let end = value.span();
            Ok(TableField::IndexField(key, value, start.merge(end)))
        } else if (self.check(TokenKind::Identifier) || self.current().kind.is_contextual_keyword()) && self.peek_ahead(1).kind == TokenKind::Assign
        {
            let name = self.expect_identifier()?;
            self.expect(TokenKind::Assign)?;
            let value = self.parse_expression()?;
            let end = value.span();
            Ok(TableField::NamedField(name, value, start.merge(end)))
        } else {
            let value = self.parse_expression()?;
            let end = value.span();
            Ok(TableField::ValueField(value, start.merge(end)))
        }
    }

    fn parse_expression_list(&mut self) -> ParseResult<Vec<Expression>> {
        let mut exprs = vec![self.parse_expression()?];
        while self.check(TokenKind::Comma) {
            self.advance();
            exprs.push(self.parse_expression()?);
        }
        Ok(exprs)
    }

    fn parse_param_list(&mut self) -> ParseResult<Vec<Parameter>> {
        let mut params = Vec::new();

        if self.check(TokenKind::RightParen) {
            return Ok(params);
        }

        if self.check(TokenKind::DotDotDot) {
            let span = self.current_span();
            self.advance();
            params.push(Parameter {
                name: Identifier {
                    name: SmolStr::new("..."),
                    span,
                },
                type_annotation: None,
                is_vararg: true,
                span,
            });
            return Ok(params);
        }

        loop {
            if self.check(TokenKind::DotDotDot) {
                let span = self.current_span();
                self.advance();
                params.push(Parameter {
                    name: Identifier {
                        name: SmolStr::new("..."),
                        span,
                    },
                    type_annotation: None,
                    is_vararg: true,
                    span,
                });
                break;
            }

            let start = self.current_span();
            let name = self.expect_identifier()?;
            let type_annotation = if self.check(TokenKind::Colon) {
                self.advance();
                Some(self.parse_type_annotation()?)
            } else {
                None
            };
            let end = self.previous_span();

            params.push(Parameter {
                name,
                type_annotation,
                is_vararg: false,
                span: start.merge(end),
            });

            if !self.check(TokenKind::Comma) {
                break;
            }
            self.advance();
        }

        Ok(params)
    }

    fn parse_type_params(&mut self) -> ParseResult<Vec<TypeParam>> {
        self.expect(TokenKind::LessThan)?;
        let mut params = Vec::new();

        loop {
            let start = self.current_span();
            let name = self.expect_identifier()?;
            let constraint = if self.check(TokenKind::Colon) {
                self.advance();
                Some(self.parse_type_annotation()?)
            } else {
                None
            };
            let end = self.previous_span();

            params.push(TypeParam {
                name,
                constraint,
                span: start.merge(end),
            });

            if !self.check(TokenKind::Comma) {
                break;
            }
            self.advance();
        }

        self.expect(TokenKind::GreaterThan)?;
        Ok(params)
    }

    fn parse_type_reference(&mut self) -> ParseResult<TypeReference> {
        let start = self.current_span();
        let name = self.expect_identifier()?;

        let type_args = if self.check(TokenKind::LessThan) {
            self.advance();
            let mut args = vec![self.parse_type_annotation()?];
            while self.check(TokenKind::Comma) {
                self.advance();
                args.push(self.parse_type_annotation()?);
            }
            self.expect(TokenKind::GreaterThan)?;
            args
        } else {
            Vec::new()
        };

        let end = self.previous_span();
        Ok(TypeReference {
            name,
            type_args,
            span: start.merge(end),
        })
    }

    fn parse_type_annotation(&mut self) -> ParseResult<TypeAnnotation> {
        let start = self.current_span();
        let mut ty = self.parse_single_type()?;

        if self.check(TokenKind::Pipe) {
            let mut types = vec![ty];
            while self.check(TokenKind::Pipe) {
                self.advance();
                types.push(self.parse_single_type()?);
            }
            let end = self.previous_span();
            ty = TypeAnnotation {
                kind: TypeKind::Union(types),
                span: start.merge(end),
            };
        }

        Ok(ty)
    }

    fn parse_single_type(&mut self) -> ParseResult<TypeAnnotation> {
        let start = self.current_span();

        let mut ty = match self.current().kind {
            TokenKind::LeftBrace => {
                self.advance();
                if self.check(TokenKind::LeftBracket) {
                    // {[K]: V} dictionary type
                    self.advance();
                    let key_type = self.parse_type_annotation()?;
                    self.expect(TokenKind::RightBracket)?;
                    self.expect(TokenKind::Colon)?;
                    let val_type = self.parse_type_annotation()?;
                    self.expect(TokenKind::RightBrace)?;
                    let end = self.previous_span();
                    TypeAnnotation {
                        kind: TypeKind::Table(Box::new(key_type), Box::new(val_type)),
                        span: start.merge(end),
                    }
                } else {
                    // {T} array-like table type
                    let inner = self.parse_type_annotation()?;
                    self.expect(TokenKind::RightBrace)?;
                    let end = self.previous_span();
                    TypeAnnotation {
                        kind: TypeKind::Array(Box::new(inner)),
                        span: start.merge(end),
                    }
                }
            }
            TokenKind::Nil => {
                self.advance();
                TypeAnnotation {
                    kind: TypeKind::Nil,
                    span: start,
                }
            }
            TokenKind::LeftParen => {
                self.advance();
                let mut types = Vec::new();
                if !self.check(TokenKind::RightParen) {
                    types.push(self.parse_type_annotation()?);
                    while self.check(TokenKind::Comma) {
                        self.advance();
                        types.push(self.parse_type_annotation()?);
                    }
                }
                self.expect(TokenKind::RightParen)?;
                if self.check(TokenKind::Arrow) {
                    self.advance();
                    let return_type = self.parse_type_annotation()?;
                    let end = self.previous_span();
                    TypeAnnotation {
                        kind: TypeKind::Function(types, Box::new(return_type)),
                        span: start.merge(end),
                    }
                } else {
                    let end = self.previous_span();
                    TypeAnnotation {
                        kind: TypeKind::Tuple(types),
                        span: start.merge(end),
                    }
                }
            }
            TokenKind::Identifier => {
                let name = self.expect_identifier()?;

                if name.name.as_str() == "any" {
                    TypeAnnotation {
                        kind: TypeKind::Any,
                        span: name.span,
                    }
                } else {
                    let type_args = if self.check(TokenKind::LessThan) {
                        self.advance();
                        let mut args = vec![self.parse_type_annotation()?];
                        while self.check(TokenKind::Comma) {
                            self.advance();
                            args.push(self.parse_type_annotation()?);
                        }
                        self.expect(TokenKind::GreaterThan)?;
                        args
                    } else {
                        Vec::new()
                    };

                    let end = self.previous_span();
                    TypeAnnotation {
                        kind: TypeKind::Named(name, type_args),
                        span: start.merge(end),
                    }
                }
            }
            _ => {
                return Err(self.error(
                    &format!("expected type, found '{}'", self.current().kind),
                    ParseErrorKind::InvalidTypeAnnotation,
                ));
            }
        };

        if self.check(TokenKind::QuestionMark) {
            self.advance();
            let end = self.previous_span();
            ty = TypeAnnotation {
                kind: TypeKind::Optional(Box::new(ty)),
                span: start.merge(end),
            };
        }

        Ok(ty)
    }

    fn current(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn current_span(&self) -> Span {
        self.current().span
    }

    fn previous_span(&self) -> Span {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span
        } else {
            self.current_span()
        }
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.current().kind == kind
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.pos += 1;
        }
        &self.tokens[self.pos - 1]
    }

    fn is_at_end(&self) -> bool {
        self.current().kind == TokenKind::Eof
    }

    /// Check if the current token can start an expression (used for optional yield value).
    fn can_start_expression(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Identifier
                | TokenKind::Number
                | TokenKind::StringLiteral
                | TokenKind::LeftParen
                | TokenKind::Minus
                | TokenKind::Not
                | TokenKind::Hash
                | TokenKind::Tilde
                | TokenKind::LeftBrace
                | TokenKind::LeftBracket
                | TokenKind::Function
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Nil
                | TokenKind::DotDotDot
                | TokenKind::Super
                | TokenKind::New
                | TokenKind::If
                | TokenKind::Await
                | TokenKind::Yield
                | TokenKind::Async
                | TokenKind::Generator
        ) || (self.current().kind.is_contextual_keyword()
            && !matches!(
                self.current().kind,
                TokenKind::End | TokenKind::Else | TokenKind::ElseIf | TokenKind::Until
            ))
    }

    fn expect(&mut self, kind: TokenKind) -> ParseResult<&Token> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(self.error(
                &format!("expected '{}', found '{}'", kind, self.current().kind),
                ParseErrorKind::UnexpectedToken {
                    expected: kind.to_string(),
                    found: self.current().kind,
                },
            ))
        }
    }

    fn expect_identifier(&mut self) -> ParseResult<Identifier> {
        if self.check(TokenKind::Identifier) || self.current().kind.is_contextual_keyword() {
            let token = self.current().clone();
            self.advance();
            Ok(Identifier {
                name: token.lexeme,
                span: token.span,
            })
        } else {
            Err(self.error(
                &format!("expected identifier, found '{}'", self.current().kind),
                ParseErrorKind::UnexpectedToken {
                    expected: "identifier".to_string(),
                    found: self.current().kind,
                },
            ))
        }
    }

    fn expect_identifier_or_keyword(&mut self) -> ParseResult<Identifier> {
        if self.check(TokenKind::Identifier) || self.current().kind.is_keyword() {
            let token = self.current().clone();
            self.advance();
            Ok(Identifier {
                name: token.lexeme,
                span: token.span,
            })
        } else {
            Err(self.error(
                &format!("expected identifier, found '{}'", self.current().kind),
                ParseErrorKind::UnexpectedToken {
                    expected: "identifier".to_string(),
                    found: self.current().kind,
                },
            ))
        }
    }

    fn peek_ahead(&self, n: usize) -> &Token {
        let idx = (self.pos + n).min(self.tokens.len() - 1);
        &self.tokens[idx]
    }

    fn skip_semicolons(&mut self) {
        while self.check(TokenKind::Semicolon) {
            self.advance();
        }
    }

    fn error(&self, message: &str, kind: ParseErrorKind) -> ParseError {
        ParseError {
            message: message.to_string(),
            span: self.current_span(),
            kind,
        }
    }

    fn error_at(&self, span: Span, message: &str, kind: ParseErrorKind) -> ParseError {
        ParseError {
            message: message.to_string(),
            span,
            kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(source: &str) -> SourceFile {
        let (file, errors) = Parser::new(source).parse();
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        file
    }

    #[test]
    fn test_empty() {
        let file = parse_ok("");
        assert!(file.statements.is_empty());
    }

    #[test]
    fn test_local_assignment() {
        let file = parse_ok("local x = 42");
        assert_eq!(file.statements.len(), 1);
        assert!(matches!(
            &file.statements[0],
            Statement::LocalAssignment(_)
        ));
    }

    #[test]
    fn test_class_basic() {
        let file = parse_ok(
            "class Foo\n\
                 public x: number\n\
                 new(x: number)\n\
                     self.x = x\n\
                 end\n\
                 public function bar(): string\n\
                     return \"hello\"\n\
                 end\n\
             end",
        );
        assert_eq!(file.statements.len(), 1);
        if let Statement::ClassDecl(class) = &file.statements[0] {
            assert_eq!(class.name.name.as_str(), "Foo");
            assert_eq!(class.members.len(), 3);
        } else {
            panic!("expected ClassDecl");
        }
    }

    #[test]
    fn test_class_inheritance() {
        let file = parse_ok(
            "class Dog extends Animal\n\
                 public function speak(): string\n\
                     return \"Woof\"\n\
                 end\n\
             end",
        );
        if let Statement::ClassDecl(class) = &file.statements[0] {
            assert!(class.parent.is_some());
            assert_eq!(class.parent.as_ref().unwrap().name.name.as_str(), "Animal");
        } else {
            panic!("expected ClassDecl");
        }
    }

    #[test]
    fn test_interface() {
        let file = parse_ok(
            "interface Drawable\n\
                 function draw(x: number, y: number): void\n\
             end",
        );
        assert!(matches!(&file.statements[0], Statement::InterfaceDecl(_)));
    }

    #[test]
    fn test_enum() {
        let file = parse_ok(
            "enum Color\n\
                 Red = 1\n\
                 Green = 2\n\
                 Blue = 3\n\
             end",
        );
        if let Statement::EnumDecl(e) = &file.statements[0] {
            assert_eq!(e.name.name.as_str(), "Color");
            assert_eq!(e.variants.len(), 3);
        } else {
            panic!("expected EnumDecl");
        }
    }

    #[test]
    fn test_abstract_class() {
        let file = parse_ok(
            "abstract class Shape\n\
                 abstract function area(): number\n\
             end",
        );
        if let Statement::ClassDecl(class) = &file.statements[0] {
            assert!(class.is_abstract);
        } else {
            panic!("expected ClassDecl");
        }
    }

    #[test]
    fn test_sealed_class() {
        let file = parse_ok(
            "sealed class Config\n\
                 public name: string\n\
             end",
        );
        if let Statement::ClassDecl(class) = &file.statements[0] {
            assert!(class.is_sealed);
        } else {
            panic!("expected ClassDecl");
        }
    }

    #[test]
    fn test_instanceof() {
        let file = parse_ok("local x = y instanceof Foo");
        assert_eq!(file.statements.len(), 1);
    }

    #[test]
    fn test_new_expression() {
        let file = parse_ok("local x = new Foo(1, 2)");
        assert_eq!(file.statements.len(), 1);
    }

    #[test]
    fn test_super_access() {
        let file = parse_ok(
            "class Dog extends Animal\n\
                 public function speak(): string\n\
                     return super.speak() .. \" woof\"\n\
                 end\n\
             end",
        );
        assert_eq!(file.statements.len(), 1);
    }

    #[test]
    fn test_if_statement() {
        let file = parse_ok("if x > 0 then return x end");
        assert!(matches!(&file.statements[0], Statement::IfStatement(_)));
    }

    #[test]
    fn test_while_statement() {
        let file = parse_ok("while true do break end");
        assert!(matches!(
            &file.statements[0],
            Statement::WhileStatement(_)
        ));
    }

    #[test]
    fn test_for_numeric() {
        let file = parse_ok("for i = 1, 10 do end");
        assert!(matches!(&file.statements[0], Statement::ForNumeric(_)));
    }

    #[test]
    fn test_for_generic() {
        let file = parse_ok("for k, v in pairs(t) do end");
        assert!(matches!(&file.statements[0], Statement::ForGeneric(_)));
    }

    #[test]
    fn test_function_decl() {
        let file = parse_ok("function foo(x, y) return x + y end");
        assert!(matches!(&file.statements[0], Statement::FunctionDecl(_)));
    }

    #[test]
    fn test_table_constructor() {
        let file = parse_ok("local t = { x = 1, y = 2, \"hello\" }");
        assert_eq!(file.statements.len(), 1);
    }

    #[test]
    fn test_generic_class() {
        let file = parse_ok(
            "class Stack<T>\n\
                 private items: {T}\n\
             end",
        );
        if let Statement::ClassDecl(class) = &file.statements[0] {
            assert_eq!(class.type_params.len(), 1);
        } else {
            panic!("expected ClassDecl");
        }
    }

    #[test]
    fn test_implements() {
        let file = parse_ok(
            "class Circle implements Drawable, Serializable\n\
                 public function draw(x: number, y: number): void\n\
                 end\n\
             end",
        );
        if let Statement::ClassDecl(class) = &file.statements[0] {
            assert_eq!(class.interfaces.len(), 2);
        } else {
            panic!("expected ClassDecl");
        }
    }
}
