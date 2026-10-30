use alloc::vec;

use super::Error;
use crate::__naga::arena::{Arena, Handle};

/// Create a default value for an output built-in.
pub fn generate_default_built_in(
    built_in: Option<crate::__naga::BuiltIn>,
    ty: Handle<crate::__naga::Type>,
    global_expressions: &mut Arena<crate::__naga::Expression>,
    span: crate::__naga::Span,
) -> Result<Handle<crate::__naga::Expression>, Error> {
    let expr = match built_in {
        Some(crate::__naga::BuiltIn::Position { .. }) => {
            let zero = global_expressions.append(
                crate::__naga::Expression::Literal(crate::__naga::Literal::F32(0.0)),
                span,
            );
            let one = global_expressions.append(
                crate::__naga::Expression::Literal(crate::__naga::Literal::F32(1.0)),
                span,
            );
            crate::__naga::Expression::Compose {
                ty,
                components: vec![zero, zero, zero, one],
            }
        }
        Some(crate::__naga::BuiltIn::PointSize) => {
            crate::__naga::Expression::Literal(crate::__naga::Literal::F32(1.0))
        }
        Some(crate::__naga::BuiltIn::FragDepth) => {
            crate::__naga::Expression::Literal(crate::__naga::Literal::F32(0.0))
        }
        Some(crate::__naga::BuiltIn::SampleMask) => {
            crate::__naga::Expression::Literal(crate::__naga::Literal::U32(u32::MAX))
        }
        // Note: `crate::BuiltIn::ClipDistance` is intentionally left for the default path
        _ => crate::__naga::Expression::ZeroValue(ty),
    };
    Ok(global_expressions.append(expr, span))
}
