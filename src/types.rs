//! Built-in type relationships. Value conversions remain separate from subtyping.
use crate::model::Type;

mod conversions;
mod display;
pub(crate) mod graph;
mod joins;
mod overlap;
mod subtyping;

impl Type {
    pub fn named(name: &str, args: Vec<Type>) -> Self {
        Self::Named {
            name: name.into(),
            args: crate::type_storage::type_list(args),
        }
    }
    pub fn parameter(name: impl Into<String>) -> Self {
        Self::Parameter {
            name: name.into(),
            nonnull: false,
        }
    }
    /// Preserve a generic parameter's identity while excluding its null value.
    pub fn non_null(&self) -> Self {
        match self {
            Self::Null => Self::Nothing,
            Self::Nullable { inner } => inner.non_null(),
            Self::Parameter { name, .. } if self.permits_null() => Self::Parameter {
                name: name.clone(),
                nonnull: true,
            },
            Self::Star => Self::Any,
            _ => self.clone(),
        }
    }
    pub fn nullable(&self) -> Self {
        match self {
            Self::Nullable { .. } | Self::Null => self.clone(),
            Self::Parameter { name, nonnull: true } => Self::Nullable {
                inner: Box::new(Self::parameter(name)),
            },
            _ => Self::Nullable {
                inner: Box::new(self.clone()),
            },
        }
    }
    pub fn present(&self) -> &Self {
        if let Self::Nullable { inner } = self {
            inner
        } else {
            self
        }
    }
    pub fn permits_null(&self) -> bool {
        match self {
            Self::Parameter { name, nonnull } => !nonnull && !crate::generics::nonnull(name),
            Self::Null | Self::Nullable { .. } | Self::Star => true,
            _ => false,
        }
    }
    pub fn collection(&self) -> Option<(&str, &[Type], bool)> {
        let Self::Named { name, args } = self else {
            return None;
        };
        let base = name.strip_prefix("Mutable").unwrap_or(name);
        ["List", "Set", "Map"]
            .contains(&base)
            .then_some((base, args, name.starts_with("Mutable")))
    }
    // 0 identity, 1 widening, 2 family copy, 3 copy plus widening.
    pub fn conversion_from(&self, source: &Self) -> Option<u8> {
        conversions::rank(self, source)
    }
    pub fn join(&self, other: &Self) -> Option<Self> {
        joins::common(self, other)
    }
    pub fn equatable(&self) -> bool {
        match self {
            Self::Parameter { name, .. } => crate::generics::capability(name, false),
            Self::Any | Self::Star | Self::Function { .. } | Self::Pending => false,
            Self::Nullable { inner } => inner.equatable(),
            Self::Named { .. } => crate::declarations::capability(self, false),
            _ => true,
        }
    }
    pub fn stable_key(&self) -> bool {
        match self {
            Self::Parameter { name, .. } => crate::generics::capability(name, true),
            Self::Named { .. } => crate::declarations::capability(self, true),
            Self::Nullable { inner } => inner.stable_key(),
            _ => self.equatable(),
        }
    }
    /// A projection hides a type argument, never a value type shared by two
    /// unrelated containers. Reads expose Any? until narrowed by a type test.
    pub fn projected_read(&self) -> Self {
        if *self == Self::Star {
            Self::Any.nullable()
        } else {
            self.clone()
        }
    }
    pub fn item_type(&self) -> Option<Self> {
        match self {
            Self::Parameter { .. } if !self.permits_null() => crate::generics::member_view(self).item_type(),
            Self::String => Some(Self::Char),
            Self::Named { name, args } if ["IntRange", "IntProgression"].contains(&name.as_str()) => Some(Self::Int),
            _ => self.collection().map(|(family, args, _)| {
                if family == "Map" {
                    Self::named("Entry", args.to_vec())
                } else {
                    args[0].projected_read()
                }
            }),
        }
    }
}
