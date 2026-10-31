use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

// Version 3 requires ordered operand checking. Older typed bodies must be
// regenerated; integrity hashes cannot establish the producer's static checks.
pub const FORMAT: &str = "bifoil.module.v3";
pub const PROFILE: &str = "bifoil-vm.v2";
pub const MAX_SOURCE: usize = 1024 * 1024;
pub const MAX_ARTIFACT: u64 = 16 * 1024 * 1024;
// Declaration interfaces remain independently bounded per source/module.
pub const MAX_FUNCTIONS: usize = 16_384;
pub const MAX_PROGRAM_FUNCTIONS: usize = 81_920;
// Each independently compiled module retains a bounded instruction body.
pub const MAX_INSTRUCTIONS: usize = 100_000;
// Packages may combine several modules without retaining their bodies in RAM.
pub const MAX_PROGRAM_INSTRUCTIONS: usize = 500_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Type {
    Int,
    Float,
    Boolean,
    String,
    Unit,
    Char,
    Null,
    Nothing,
    Any,
    Star,
    Nullable {
        inner: Box<Type>,
    },
    Named {
        name: String,
        #[serde(deserialize_with = "crate::type_storage::deserialize_types")]
        args: Arc<[Type]>,
    },
    Parameter {
        name: String,
        #[serde(default, skip_serializing_if = "is_false")]
        nonnull: bool,
    },
    Scene {
        #[serde(deserialize_with = "crate::type_storage::deserialize_types")]
        params: Arc<[Type]>,
    },
    #[serde(skip)]
    Pending,
    Function {
        #[serde(deserialize_with = "crate::type_storage::deserialize_types")]
        params: Arc<[Type]>,
        #[serde(deserialize_with = "crate::type_storage::deserialize_result")]
        result: Arc<Type>,
        suspend: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        vararg: bool,
    },
}
impl Type {
    pub fn accepts(&self, other: &Self) -> bool {
        self.conversion_from(other).is_some()
    }
    pub fn renderable(&self) -> bool {
        if matches!(self, Self::Parameter { .. }) && !self.permits_null() {
            return crate::generics::operation_view(self).renderable();
        }
        matches!(
            self,
            Self::Int | Self::Float | Self::Boolean | Self::String | Self::Char | Self::Nothing
        )
    }
}
fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub file: Arc<str>,
    pub line: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub mutable: bool,
    pub default: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub vararg: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenericParam {
    pub name: String,
    pub variance: i8,
    pub bounds: Vec<Type>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignatureData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generics: Vec<GenericParam>,
    pub id: String,
    /// Symbol and interface views share immutable parameters; specialization detaches changed lists.
    #[serde(deserialize_with = "crate::type_storage::deserialize_parameters")]
    pub params: Arc<[Param]>,
    pub result: Type,
    pub suspend: bool,
    pub scene: bool,
}
/// Callable views share immutable headers; inference and specialization detach changes.
pub type Signature = Arc<SignatureData>;

impl SignatureData {
    /// Detach a shared parameter list only when a private header changes it.
    pub(crate) fn params_mut(&mut self) -> &mut [Param] {
        // There are no elements to change; keep the shared empty allocation.
        if self.params.is_empty() {
            return &mut [];
        }
        Arc::make_mut(&mut self.params)
    }
    pub fn ty(&self) -> Type {
        let params = crate::type_storage::type_list(self.params.iter().map(|p| p.ty.clone()).collect());
        if self.scene {
            return Type::Scene { params };
        }
        Type::Function {
            params,
            result: crate::type_storage::result(&self.result),
            suspend: self.suspend,
            vararg: self.params.last().is_some_and(|p| p.vararg),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function<C = Vec<Instruction>> {
    pub signature: Signature,
    pub code: C,
    pub init: bool,
    #[serde(default)]
    pub local: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instruction {
    #[serde(flatten)]
    pub op: Op,
    pub span: Span,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Unreachable,
    LegacyCall {
        target: LegacyTarget,
        arguments: usize,
    },
    Constant {
        ty: Type,
        value: Value,
    },
    Load {
        name: String,
    },
    Declare {
        name: String,
        ty: Type,
        mutable: bool,
    },
    Store {
        name: String,
    },
    Uninitialized {
        name: String,
        ty: Type,
        mutable: bool,
    },
    Closure {
        name: String,
        function: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        captures: Option<Vec<String>>,
    },
    Pop,
    Dup,
    Widen,
    Convert {
        ty: Type,
    },
    Member {
        name: String,
        ty: Type,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        function: Option<String>,
    },
    Index,
    AssertPresent,
    TypeTest {
        ty: Type,
        negate: bool,
    },
    Cast {
        ty: Type,
        safe: bool,
    },
    JumpNull {
        target: usize,
        when_null: bool,
    },
    BeginCollection {
        ty: Type,
    },
    CollectionAdd,
    CheckKey,
    CollectionPut,
    AddressMember {
        name: String,
    },
    AddressIndex,
    ReadAddress,
    WriteAddress,
    Range {
        descending: bool,
        exclusive: bool,
    },
    RangeStep,
    IterStart {
        id: usize,
    },
    IterNext {
        id: usize,
        name: String,
        ty: Type,
        target: usize,
    },
    IterEnd {
        id: usize,
    },
    StackMark {
        id: usize,
    },
    StackReset {
        id: usize,
    },
    StackEnd {
        id: usize,
    },
    Unwind {
        count: usize,
    },
    Unused {
        site: String,
    },
    SceneCall {
        names: Vec<Option<String>>,
        direct: bool,
        transfer: bool,
    },
    Unary {
        operator: String,
    },
    Binary {
        operator: String,
    },
    Jump {
        target: usize,
    },
    JumpFalse {
        target: usize,
    },
    JumpTrue {
        target: usize,
    },
    Enter,
    Exit,
    Default {
        name: String,
        target: usize,
    },
    Specialize {
        args: BTreeMap<String, Type>,
    },
    Call {
        names: Vec<Option<String>>,
        direct: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        spreads: Vec<bool>,
    },
    Return,
    Format {
        count: usize,
    },
    Print,
    Construct {
        ty: Type,
        fields: Vec<String>,
        tag: Option<String>,
    },
    ProseUnwind {
        depth: usize,
    },
    ProseStart,
    ProseText,
    ProseFlush,
    ProseEnd,
    FunctionValue {
        function: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        captures: Option<Vec<String>>,
    },
    BeginChoice,
    Offer {
        target: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        once: Option<String>,
    },
    Choose {
        fallback: Option<usize>,
    },
}
impl Op {
    /// A local instruction address, rather than a function, iterator, or scene ID.
    pub(crate) fn instruction_target(&self) -> Option<usize> {
        match self {
            Self::Jump { target }
            | Self::JumpFalse { target }
            | Self::JumpTrue { target }
            | Self::JumpNull { target, .. }
            | Self::Default { target, .. }
            | Self::Offer { target, .. }
            | Self::IterNext { target, .. } => Some(*target),
            Self::Choose { fallback } => *fallback,
            _ => None,
        }
    }

    pub(crate) fn instruction_target_mut(&mut self) -> Option<&mut usize> {
        match self {
            Self::Jump { target }
            | Self::JumpFalse { target }
            | Self::JumpTrue { target }
            | Self::JumpNull { target, .. }
            | Self::Default { target, .. }
            | Self::Offer { target, .. }
            | Self::IterNext { target, .. } => Some(target),
            Self::Choose { fallback } => fallback.as_mut(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Export {
    Function { signature: Signature },
    Functions { signatures: Vec<Signature> },
    Global { id: String, ty: Type, mutable: bool },
    Type { id: String, constructor: Option<Signature> },
    Alias { parameters: Vec<GenericParam>, ty: Type },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDef {
    pub ty: Type,
    pub mutable: bool,
    pub private: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeDef {
    pub parameters: Vec<GenericParam>,
    pub field_order: Vec<String>,
    pub kind: String,
    pub fields: BTreeMap<String, FieldDef>,
    pub methods: BTreeMap<String, Vec<Signature>>,
    pub private_methods: Vec<String>,
    pub parent: Option<Type>,
    pub variants: Vec<String>,
}
/// Completed declarations are immutable across module and analysis views.
/// Arc serializes transparently, preserving the existing artifact representation.
pub type TypeDefinitions = BTreeMap<String, Arc<TypeDef>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyTarget {
    #[serde(default, skip_serializing_if = "is_false")]
    pub transfer: bool,
    pub scene: String,
    pub label: Option<String>,
    pub result: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyGlobal {
    pub ty: Type,
    pub mutable: bool,
    pub target: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedDef {
    pub dependencies: Arc<[String]>,
    pub order: usize,
    pub ty: Type,
    pub mutable: bool,
    pub initializer: String,
}
/// A strongly connected component is fingerprinted as a whole, without circular hashes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleGroup {
    pub members: Vec<String>,
    pub digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Module<F = BTreeMap<String, Function>> {
    #[serde(skip)]
    pub summaries: BTreeMap<String, Arc<FunctionSummary>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<ModuleGroup>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub legacy_globals: BTreeMap<String, LegacyGlobal>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub shared: BTreeMap<String, SharedDef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub types: TypeDefinitions,
    pub format: String,
    pub profile: String,
    pub name: String,
    pub digest: String,
    pub dependencies: BTreeMap<String, String>,
    pub exports: BTreeMap<String, Export>,
    pub functions: F,
    pub init: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program<F = BTreeMap<String, Function>> {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub legacy_globals: BTreeMap<String, LegacyGlobal>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub legacy_sources: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_order: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub shared: BTreeMap<String, SharedDef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub types: TypeDefinitions,
    pub format: String,
    pub profile: String,
    pub build: String,
    pub entry: String,
    pub modules: BTreeMap<String, String>,
    pub functions: F,
    pub init: Vec<String>,
}
impl<F> Program<F> {
    pub(crate) fn with_functions<G>(self, functions: G) -> Program<G> {
        Program {
            legacy_globals: self.legacy_globals,
            legacy_sources: self.legacy_sources,
            shared_order: self.shared_order,
            shared: self.shared,
            types: self.types,
            format: self.format,
            profile: self.profile,
            build: self.build,
            entry: self.entry,
            modules: self.modules,
            functions,
            init: self.init,
        }
    }
}
pub(crate) use crate::artifact::digest;
pub fn unit() -> Op {
    Op::Constant {
        ty: Type::Unit,
        value: Value::Null,
    }
}

/// Kept only while compiling/linking; these summaries are not part of module JSON.
#[derive(Debug, Clone, Default)]
pub struct FunctionSummary {
    pub error: Option<String>,
    pub bindings: BTreeSet<String>,
    pub calls: BTreeSet<String>,
    pub indirect_call: bool,
}
impl FunctionSummary {
    /// Pure bodies share one immutable empty effect record. Nonempty summaries
    /// retain their own dependencies and analysis failures.
    pub(crate) fn shared(self) -> Arc<Self> {
        if self.error.is_none() && self.bindings.is_empty() && self.calls.is_empty() && !self.indirect_call {
            static EMPTY: std::sync::LazyLock<Arc<FunctionSummary>> =
                std::sync::LazyLock::new(|| Arc::new(FunctionSummary::default()));
            Arc::clone(&EMPTY)
        } else {
            Arc::new(self)
        }
    }
}
impl<F> Module<F> {
    /// Preserve the canonical module field order while changing only its code source.
    pub(crate) fn with_functions<G>(self, functions: G) -> Module<G> {
        Module {
            summaries: self.summaries,
            group: self.group,
            legacy_globals: self.legacy_globals,
            shared: self.shared,
            types: self.types,
            format: self.format,
            profile: self.profile,
            name: self.name,
            digest: self.digest,
            dependencies: self.dependencies,
            exports: self.exports,
            functions,
            init: self.init,
        }
    }
}

/// A dependency scope borrows metadata; it never owns or clones compiled modules.
pub(crate) type ModuleView<'a> = BTreeMap<String, &'a Module>;
