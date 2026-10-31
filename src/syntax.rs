//! Source AST shared by parsing, declaration resolution, and semantic analysis.

use crate::model::{GenericParam, LegacyTarget, Type};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Expr {
    TypeApply {
        callee: Box<Expr>,
        args: Vec<Type>,
    },
    Construct {
        ty: Type,
        fields: Vec<(String, Expr)>,
        tag: Option<String>,
    },
    Block(Vec<Statement>),
    Lambda(Box<FunctionAst>),
    Int(i64),
    Float(f64),
    Boolean(bool),
    String(String),
    Char(String),
    Null,
    Collection {
        family: Option<String>,
        mutable: bool,
        entries: Vec<(Option<Expr>, Expr)>,
    },
    Member {
        receiver: Box<Expr>,
        name: String,
        safe: bool,
    },
    Index {
        receiver: Box<Expr>,
        index: Box<Expr>,
        safe: bool,
    },
    Unwrap(Box<Expr>),
    Spread(Box<Expr>),
    SafeCall(Box<Expr>, Vec<(Option<String>, Expr)>),
    BoundReference(Box<Expr>, String),
    TypeOp {
        value: Box<Expr>,
        operator: String,
        ty: Type,
    },
    Transfer {
        kind: String,
        label: Option<String>,
        value: Option<Box<Expr>>,
    },
    Format(Vec<TemplatePart>),
    Name(String),
    Reference(String),
    Group(Box<Expr>),
    Unary(String, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<(Option<String>, Expr)>),
}
/// An interpolation retains its physical origin even after paragraph folding.
#[derive(Debug, Clone)]
pub struct TemplatePart {
    pub value: Expr,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub ty: Type,
    pub mutable: bool,
    pub vararg: bool,
    pub default: Option<Expr>,
}
#[derive(Debug, Clone)]
pub struct FunctionAst {
    pub legacy: Option<LegacyTarget>,
    pub generics: Vec<GenericParam>,
    pub name: String,
    /// Normalized clones share the header; rewriting annotations or defaults uses copy-on-write.
    pub params: Arc<[Parameter]>,
    pub result: Option<Type>,
    pub suspend: bool,
    pub scene: bool,
    pub exported: bool,
    pub body: Vec<Statement>,
    pub expression: bool,
    pub line: usize,
}
#[derive(Debug, Clone)]
pub struct OptionAst {
    pub label: Expr,
    pub visible: Expr,
    pub visible_line: usize,
    pub enabled: Expr,
    pub enabled_line: usize,
    pub body: Vec<Statement>,
    pub once: bool,
    pub line: usize,
}
#[derive(Debug, Clone)]
pub enum Pattern {
    Value(Expr),
    Membership {
        value: Expr,
        negate: bool,
    },
    Type {
        ty: Type,
        bindings: Option<Vec<String>>,
        negate: bool,
    },
}
/// A horizontal if/elif/else-if arm; only its body creates a lexical scope.
#[derive(Debug, Clone)]
pub struct ConditionalBranch {
    pub condition: Expr,
    pub body: Vec<Statement>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct WhenCase {
    pub patterns: Vec<Pattern>,
    pub guard: Option<Expr>,
    pub body: Vec<Statement>,
    pub fallback: bool,
    pub line: usize,
}
#[derive(Debug, Clone)]
pub enum StatementKind {
    Alias(AliasAst),
    When {
        subject: Option<Expr>,
        binding: Option<String>,
        annotation: Option<Type>,
        cases: Vec<WhenCase>,
    },
    Yield {
        value: Expr,
        expected: Type,
    },
    Narrative(Vec<Statement>),
    Text(Expr),
    ParagraphBreak,
    Uninitialized {
        name: String,
        ty: Type,
        mutable: bool,
    },
    Write {
        target: Expr,
        operator: String,
        value: Expr,
    },
    For {
        label: Option<String>,
        name: String,
        iterable: Expr,
        body: Vec<Statement>,
    },
    While {
        label: Option<String>,
        condition: Expr,
        body: Vec<Statement>,
        post: bool,
    },
    Variable {
        name: String,
        ty: Option<Type>,
        mutable: bool,
        value: Expr,
    },
    Assign {
        name: String,
        operator: String,
        value: Expr,
    },
    Expression(Expr),
    Return(Option<Expr>),
    Function(FunctionAst),
    If {
        branches: Vec<ConditionalBranch>,
        no: Vec<Statement>,
    },
    Choice {
        options: Vec<OptionAst>,
        fallback: Option<Vec<Statement>>,
    },
}
#[derive(Debug, Clone)]
pub struct Statement {
    pub kind: StatementKind,
    pub line: usize,
}
#[derive(Debug, Clone)]
pub struct Global {
    pub stmt: Statement,
    pub exported: bool,
}
#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub symbol: String,
    pub alias: String,
}
#[derive(Debug, Clone)]
pub struct Ast {
    pub legacy_globals: Vec<LegacyGlobalAst>,
    pub aliases: Vec<AliasAst>,
    pub types: Vec<TypeAst>,
    pub imports: Vec<Import>,
    pub globals: Vec<Global>,
    pub functions: Vec<FunctionAst>,
}

#[derive(Debug, Clone)]
pub struct FieldAst {
    pub name: String,
    pub ty: Type,
    pub mutable: bool,
    pub private: bool,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct TypeAst {
    pub generics: Vec<GenericParam>,
    pub name: String,
    pub kind: String,
    pub exported: bool,
    pub fields: Vec<FieldAst>,
    pub methods: Vec<FunctionAst>,
    pub variants: Vec<TypeAst>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct AliasAst {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub ty: Type,
    pub exported: bool,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct LegacyGlobalAst {
    pub name: String,
    pub ty: Type,
    pub mutable: bool,
    pub target: String,
    pub exported: bool,
    pub line: usize,
}
