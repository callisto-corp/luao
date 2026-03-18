use crate::symbol::SymbolId;

#[derive(Debug, Clone, PartialEq)]
pub enum LuaoType {
    Number,
    String,
    Boolean,
    Nil,
    Any,
    Void,
    Table(Box<LuaoType>, Box<LuaoType>),
    Array(Box<LuaoType>),
    Function(Vec<LuaoType>, Box<LuaoType>),
    Class(SymbolId),
    Interface(SymbolId),
    Enum(SymbolId),
    Union(Vec<LuaoType>),
    Optional(Box<LuaoType>),
    TypeParam(String),
    Unknown,
}
