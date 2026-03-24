use luao_lexer::Span;
use smol_str::SmolStr;

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub statements: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Identifier {
    pub name: SmolStr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Statement {
    ClassDecl(ClassDecl),
    InterfaceDecl(InterfaceDecl),
    EnumDecl(EnumDecl),
    TypeAlias(TypeAliasDecl),
    ImportDecl(ImportDecl),
    ExportDecl(Box<Statement>, Span),
    LocalAssignment(LocalAssignment),
    Assignment(Assignment),
    CompoundAssignment(CompoundAssignment),
    FunctionDecl(FunctionDecl),
    IfStatement(IfStatement),
    WhileStatement(WhileStatement),
    RepeatStatement(RepeatStatement),
    ForNumeric(ForNumericStatement),
    ForGeneric(ForGenericStatement),
    DoBlock(Block),
    SwitchStatement(SwitchStatement),
    ReturnStatement(ReturnStatement),
    Break(Span),
    Continue(Span),
    ExpressionStatement(Expression),
}

#[derive(Debug, Clone)]
pub struct ImportDecl {
    pub names: Vec<ImportName>,
    pub path: SmolStr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImportName {
    pub name: Identifier,
    pub alias: Option<Identifier>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeAliasDecl {
    pub name: Identifier,
    pub type_params: Vec<TypeParam>,
    pub value: TypeAnnotation,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ClassDecl {
    pub name: Identifier,
    pub type_params: Vec<TypeParam>,
    pub parent: Option<TypeReference>,
    pub interfaces: Vec<TypeReference>,
    pub is_abstract: bool,
    pub is_sealed: bool,
    pub is_extern: bool,
    pub members: Vec<ClassMember>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ClassMember {
    Field(FieldDecl),
    Method(MethodDecl),
    Constructor(ConstructorDecl),
    Property(PropertyDecl),
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: Identifier,
    pub type_annotation: Option<TypeAnnotation>,
    pub default_value: Option<Expression>,
    pub access: AccessModifier,
    pub is_static: bool,
    pub is_readonly: bool,
    pub is_extern: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MethodDecl {
    pub name: Identifier,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Parameter>,
    pub return_type: Option<TypeAnnotation>,
    pub body: Option<Block>,
    pub access: AccessModifier,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_override: bool,
    pub is_extern: bool,
    pub is_async: bool,
    pub is_generator: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConstructorDecl {
    pub params: Vec<Parameter>,
    pub body: Block,
    pub access: AccessModifier,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct PropertyDecl {
    pub name: Identifier,
    pub type_annotation: Option<TypeAnnotation>,
    pub getter: Option<Block>,
    pub setter: Option<(Identifier, Block)>,
    pub access: AccessModifier,
    pub is_extern: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct InterfaceDecl {
    pub name: Identifier,
    pub type_params: Vec<TypeParam>,
    pub extends: Vec<TypeReference>,
    pub members: Vec<InterfaceMember>,
    pub is_extern: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum InterfaceMember {
    Method(InterfaceMethod),
    Field(InterfaceField),
}

#[derive(Debug, Clone)]
pub struct InterfaceMethod {
    pub name: Identifier,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Parameter>,
    pub return_type: Option<TypeAnnotation>,
    pub is_extern: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct InterfaceField {
    pub name: Identifier,
    pub type_annotation: TypeAnnotation,
    pub is_extern: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: Identifier,
    pub variants: Vec<EnumVariant>,
    pub is_extern: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: Identifier,
    pub value: Option<Expression>,
    pub is_extern: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessModifier {
    Public,
    Private,
    Protected,
}

impl Default for AccessModifier {
    fn default() -> Self {
        AccessModifier::Public
    }
}

#[derive(Debug, Clone)]
pub struct TypeParam {
    pub name: Identifier,
    pub constraint: Option<TypeAnnotation>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeReference {
    pub name: Identifier,
    pub type_args: Vec<TypeAnnotation>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeAnnotation {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TypeKind {
    Named(Identifier, Vec<TypeAnnotation>),
    Function(Vec<TypeAnnotation>, Box<TypeAnnotation>),
    Array(Box<TypeAnnotation>),
    Table(Box<TypeAnnotation>, Box<TypeAnnotation>),
    Tuple(Vec<TypeAnnotation>),
    Union(Vec<TypeAnnotation>),
    Optional(Box<TypeAnnotation>),
    Nil,
    Any,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: Identifier,
    pub type_annotation: Option<TypeAnnotation>,
    pub is_vararg: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct LocalAssignment {
    pub names: Vec<Identifier>,
    pub type_annotations: Vec<Option<TypeAnnotation>>,
    pub values: Vec<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Assignment {
    pub targets: Vec<Expression>,
    pub values: Vec<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompoundOp {
    Add,    // +=
    Sub,    // -=
    Mul,    // *=
    Div,    // /=
    Mod,    // %=
    Pow,    // ^=
    Concat, // ..=
}

#[derive(Debug, Clone)]
pub struct CompoundAssignment {
    pub target: Expression,
    pub op: CompoundOp,
    pub value: Expression,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: FunctionName,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Parameter>,
    pub return_type: Option<TypeAnnotation>,
    pub body: Block,
    pub is_local: bool,
    pub is_async: bool,
    pub is_generator: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FunctionName {
    pub parts: Vec<Identifier>,
    pub method: Option<Identifier>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IfStatement {
    pub condition: Expression,
    pub then_block: Block,
    pub elseif_clauses: Vec<(Expression, Block)>,
    pub else_block: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct WhileStatement {
    pub condition: Expression,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct RepeatStatement {
    pub body: Block,
    pub condition: Expression,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ForNumericStatement {
    pub name: Identifier,
    pub start: Expression,
    pub stop: Expression,
    pub step: Option<Expression>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ForGenericStatement {
    pub names: Vec<(Identifier, Option<TypeAnnotation>)>,
    pub iterators: Vec<Expression>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SwitchStatement {
    pub subject: Expression,
    pub cases: Vec<SwitchCase>,
    pub default: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SwitchCase {
    pub values: Vec<Expression>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ReturnStatement {
    pub values: Vec<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Expression {
    Nil(Span),
    True(Span),
    False(Span),
    Number(SmolStr, Span),
    String(SmolStr, Span),
    Vararg(Span),
    Identifier(Identifier),
    BinaryOp(Box<BinaryOp>),
    UnaryOp(Box<UnaryOp>),
    FunctionCall(Box<FunctionCall>),
    MethodCall(Box<MethodCall>),
    FieldAccess(Box<FieldAccess>),
    IndexAccess(Box<IndexAccess>),
    FunctionExpr(Box<FunctionExpr>),
    TableConstructor(Box<TableConstructor>),
    Instanceof(Box<InstanceofExpr>),
    SuperAccess(Box<SuperAccess>),
    NewExpr(Box<NewExpr>),
    CastExpr(Box<CastExpr>),
    IfExpression(Box<IfExpr>),
    YieldExpr(Box<YieldExpr>),
    AwaitExpr(Box<AwaitExpr>),
    ArrayLiteral(Box<ArrayLiteral>),
    TupleLiteral(Box<TupleLiteral>),
    /// Parenthesized expression — preserves explicit `(expr)` from source.
    Grouped(Box<Expression>, Span),
}

impl Expression {
    pub fn span(&self) -> Span {
        match self {
            Expression::Nil(s) => *s,
            Expression::True(s) => *s,
            Expression::False(s) => *s,
            Expression::Number(_, s) => *s,
            Expression::String(_, s) => *s,
            Expression::Vararg(s) => *s,
            Expression::Identifier(id) => id.span,
            Expression::BinaryOp(b) => b.span,
            Expression::UnaryOp(u) => u.span,
            Expression::FunctionCall(f) => f.span,
            Expression::MethodCall(m) => m.span,
            Expression::FieldAccess(f) => f.span,
            Expression::IndexAccess(i) => i.span,
            Expression::FunctionExpr(f) => f.span,
            Expression::TableConstructor(t) => t.span,
            Expression::Instanceof(i) => i.span,
            Expression::SuperAccess(s) => s.span,
            Expression::NewExpr(n) => n.span,
            Expression::CastExpr(c) => c.span,
            Expression::IfExpression(i) => i.span,
            Expression::YieldExpr(y) => y.span,
            Expression::AwaitExpr(a) => a.span,
            Expression::ArrayLiteral(a) => a.span,
            Expression::TupleLiteral(t) => t.span,
            Expression::Grouped(_, s) => *s,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BinaryOp {
    pub left: Expression,
    pub op: BinOp,
    pub right: Expression,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Mod,
    Pow,
    Concat,
    Eq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
}

#[derive(Debug, Clone)]
pub struct UnaryOp {
    pub op: UnOp,
    pub operand: Expression,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    Len,
    BitNot,
}

#[derive(Debug, Clone)]
pub struct FunctionCall {
    pub callee: Expression,
    pub args: Vec<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MethodCall {
    pub object: Expression,
    pub method: Identifier,
    pub args: Vec<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FieldAccess {
    pub object: Expression,
    pub field: Identifier,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IndexAccess {
    pub object: Expression,
    pub index: Expression,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FunctionExpr {
    pub params: Vec<Parameter>,
    pub return_type: Option<TypeAnnotation>,
    pub body: Block,
    pub is_async: bool,
    pub is_generator: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TableConstructor {
    pub fields: Vec<TableField>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TableField {
    NamedField(Identifier, Expression, Span),
    IndexField(Expression, Expression, Span),
    ValueField(Expression, Span),
}

#[derive(Debug, Clone)]
pub struct InstanceofExpr {
    pub object: Expression,
    pub class_name: Identifier,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SuperAccess {
    pub method: Identifier,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct NewExpr {
    pub class_name: TypeReference,
    pub args: Vec<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct CastExpr {
    pub expr: Expression,
    pub target_type: TypeAnnotation,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IfExpr {
    pub condition: Expression,
    pub then_expr: Expression,
    pub elseif_clauses: Vec<(Expression, Expression)>,
    pub else_expr: Expression,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct YieldExpr {
    pub value: Option<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct AwaitExpr {
    pub expr: Expression,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ArrayLiteral {
    pub elements: Vec<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TupleLiteral {
    pub elements: Vec<Expression>,
    pub span: Span,
}
