//! Class elements: their decorators, keys and kinds.

use super::*;

pub(super) fn element_decorators<'b, 'a>(e: &'b ClassElement<'a>) -> &'b [Decorator<'a>] {
    match e {
        ClassElement::MethodDefinition(m) => &m.decorators,
        ClassElement::PropertyDefinition(p) => &p.decorators,
        ClassElement::AccessorProperty(p) => &p.decorators,
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => &[],
    }
}

pub(super) fn element_decorators_mut<'b, 'a>(
    e: &'b mut ClassElement<'a>,
) -> Option<&'b mut ArenaVec<'a, Decorator<'a>>> {
    match e {
        ClassElement::MethodDefinition(m) => Some(&mut m.decorators),
        ClassElement::PropertyDefinition(p) => Some(&mut p.decorators),
        ClassElement::AccessorProperty(p) => Some(&mut p.decorators),
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => None,
    }
}

pub(super) fn element_key<'b, 'a>(e: &'b ClassElement<'a>) -> Option<&'b PropertyKey<'a>> {
    match e {
        ClassElement::MethodDefinition(m) => Some(&m.key),
        ClassElement::PropertyDefinition(p) => Some(&p.key),
        ClassElement::AccessorProperty(p) => Some(&p.key),
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => None,
    }
}

pub(super) fn element_key_mut<'b, 'a>(
    e: &'b mut ClassElement<'a>,
) -> Option<&'b mut PropertyKey<'a>> {
    match e {
        ClassElement::MethodDefinition(m) => Some(&mut m.key),
        ClassElement::PropertyDefinition(p) => Some(&mut p.key),
        ClassElement::AccessorProperty(p) => Some(&mut p.key),
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => None,
    }
}

pub(super) fn element_computed_mut<'b>(e: &'b mut ClassElement<'_>) -> Option<&'b mut bool> {
    match e {
        ClassElement::MethodDefinition(m) => Some(&mut m.computed),
        ClassElement::PropertyDefinition(p) => Some(&mut p.computed),
        ClassElement::AccessorProperty(p) => Some(&mut p.computed),
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => None,
    }
}

pub(super) fn element_is_computed(e: &ClassElement<'_>) -> bool {
    match e {
        ClassElement::MethodDefinition(m) => m.computed,
        ClassElement::PropertyDefinition(p) => p.computed,
        ClassElement::AccessorProperty(p) => p.computed,
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => false,
    }
}

pub(super) fn element_is_static(e: &ClassElement<'_>) -> bool {
    match e {
        ClassElement::MethodDefinition(m) => m.r#static,
        ClassElement::PropertyDefinition(p) => p.r#static,
        ClassElement::AccessorProperty(p) => p.r#static,
        ClassElement::StaticBlock(_) => true,
        ClassElement::TSIndexSignature(s) => s.r#static,
    }
}

pub(super) fn element_kind(e: &ClassElement<'_>) -> ElemKind {
    match e {
        ClassElement::StaticBlock(_) => ElemKind::StaticBlock,
        ClassElement::MethodDefinition(m) => {
            if m.value.body.is_none()
                || m.r#type == MethodDefinitionType::TSAbstractMethodDefinition
            {
                return ElemKind::TypeOnly;
            }
            match m.kind {
                MethodDefinitionKind::Constructor => ElemKind::Constructor,
                MethodDefinitionKind::Method => ElemKind::Method,
                MethodDefinitionKind::Get => ElemKind::Getter,
                MethodDefinitionKind::Set => ElemKind::Setter,
            }
        }
        ClassElement::PropertyDefinition(p) => {
            if p.declare || p.r#type == PropertyDefinitionType::TSAbstractPropertyDefinition {
                ElemKind::TypeOnly
            } else {
                ElemKind::Field
            }
        }
        ClassElement::AccessorProperty(p) => {
            if p.r#type == AccessorPropertyType::TSAbstractAccessorProperty {
                ElemKind::TypeOnly
            } else {
                ElemKind::Accessor
            }
        }
        ClassElement::TSIndexSignature(_) => ElemKind::TypeOnly,
    }
}

pub(super) fn is_abstract_element(e: &ClassElement<'_>) -> bool {
    match e {
        ClassElement::MethodDefinition(m) => {
            m.r#type == MethodDefinitionType::TSAbstractMethodDefinition
        }
        ClassElement::PropertyDefinition(p) => {
            p.r#type == PropertyDefinitionType::TSAbstractPropertyDefinition
        }
        ClassElement::AccessorProperty(p) => {
            p.r#type == AccessorPropertyType::TSAbstractAccessorProperty
        }
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => false,
    }
}

/// The name a non-computed key gives an anonymous class or function (`x = class {}`).
pub(super) fn static_key_name(key: &PropertyKey<'_>, computed: bool) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(id) if !computed => Some(id.name.as_str().to_owned()),
        PropertyKey::PrivateIdentifier(p) => Some(format!("#{}", p.name)),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str().to_owned()),
        _ => None,
    }
}

/// The name esbuild derives temporaries from (`propertyNameHint`).
pub(super) fn key_hint(key: &PropertyKey<'_>) -> String {
    match key {
        PropertyKey::StaticIdentifier(id) => id.name.as_str().to_owned(),
        PropertyKey::PrivateIdentifier(p) => p.name.as_str().to_owned(),
        PropertyKey::StringLiteral(s) => s.value.as_str().to_owned(),
        PropertyKey::Identifier(id) => id.name.as_str().to_owned(),
        _ => String::new(),
    }
}

/// Whether evaluating a key has no side effects (esbuild: strings, numbers, private names).
pub(super) fn key_is_pure(key: &PropertyKey<'_>, computed: bool) -> bool {
    match key {
        PropertyKey::StaticIdentifier(_) | PropertyKey::PrivateIdentifier(_) => true,
        PropertyKey::StringLiteral(_) | PropertyKey::NumericLiteral(_) => true,
        PropertyKey::BigIntLiteral(_) => !computed,
        _ => false,
    }
}
