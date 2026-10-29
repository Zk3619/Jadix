//! Variant prefix parsing (`hover:`, `dark:`, `data-[state=open]:`, …).

use crate::catalog::is_known_utility;

/// Default Tailwind min-width breakpoints (logical px).
pub const BREAKPOINT_SM: u32 = 640;
pub const BREAKPOINT_MD: u32 = 768;
pub const BREAKPOINT_LG: u32 = 1024;
pub const BREAKPOINT_XL: u32 = 1280;
pub const BREAKPOINT_2XL: u32 = 1536;

/// One interaction / theme / data selector stacked on a utility.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum VariantSelector {
    Hover,
    Focus,
    FocusVisible,
    Active,
    Disabled,
    Dark,
    /// Responsive `sm:` / `md:` / `min-[800px]:` — matches when viewport width ≥ `min_width`.
    Breakpoint {
        min_width: u32,
    },
    /// `max-sm:` / `max-[600px]:` — matches when viewport width < `max_width`.
    MaxBreakpoint {
        max_width: u32,
    },
    /// `group-hover` / `group-hover/name` — ancestor with matching `group` is hovered.
    GroupHover {
        name: Option<String>,
    },
    /// `peer-hover` / `peer-hover/name` — previous sibling with matching `peer` is hovered.
    PeerHover {
        name: Option<String>,
    },
    /// `data-[state=open]` → key=`state`, value=`open`
    Data {
        key: String,
        value: String,
    },
    /// `motion-reduce:` — `prefers-reduced-motion` is on.
    MotionReduce,
    /// `motion-safe:` — reduced motion is off.
    MotionSafe,
}

impl VariantSelector {
    pub fn parse_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            "hover" => Some(Self::Hover),
            "focus" => Some(Self::Focus),
            "focus-visible" => Some(Self::FocusVisible),
            "active" => Some(Self::Active),
            "disabled" => Some(Self::Disabled),
            "dark" => Some(Self::Dark),
            "motion-reduce" => Some(Self::MotionReduce),
            "motion-safe" => Some(Self::MotionSafe),
            "sm" => Some(Self::Breakpoint {
                min_width: BREAKPOINT_SM,
            }),
            "md" => Some(Self::Breakpoint {
                min_width: BREAKPOINT_MD,
            }),
            "lg" => Some(Self::Breakpoint {
                min_width: BREAKPOINT_LG,
            }),
            "xl" => Some(Self::Breakpoint {
                min_width: BREAKPOINT_XL,
            }),
            "2xl" => Some(Self::Breakpoint {
                min_width: BREAKPOINT_2XL,
            }),
            "max-sm" => Some(Self::MaxBreakpoint {
                max_width: BREAKPOINT_SM,
            }),
            "max-md" => Some(Self::MaxBreakpoint {
                max_width: BREAKPOINT_MD,
            }),
            "max-lg" => Some(Self::MaxBreakpoint {
                max_width: BREAKPOINT_LG,
            }),
            "max-xl" => Some(Self::MaxBreakpoint {
                max_width: BREAKPOINT_XL,
            }),
            "max-2xl" => Some(Self::MaxBreakpoint {
                max_width: BREAKPOINT_2XL,
            }),
            "group-hover" => Some(Self::GroupHover { name: None }),
            "peer-hover" => Some(Self::PeerHover { name: None }),
            _ => parse_bracket_or_named_group(prefix),
        }
    }
}

fn parse_breakpoint_px(inner: &str) -> Option<u32> {
    let s = inner.trim();
    let s = s.strip_suffix("px").unwrap_or(s).trim();
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn parse_bracket_or_named_group(prefix: &str) -> Option<VariantSelector> {
    if let Some(name) = prefix.strip_prefix("group-hover/") {
        let name = name.trim();
        if !name.is_empty() {
            return Some(VariantSelector::GroupHover {
                name: Some(name.to_string()),
            });
        }
    }
    if let Some(name) = prefix.strip_prefix("peer-hover/") {
        let name = name.trim();
        if !name.is_empty() {
            return Some(VariantSelector::PeerHover {
                name: Some(name.to_string()),
            });
        }
    }
    if let Some(inner) = prefix.strip_prefix("min-[") {
        let px = parse_breakpoint_px(inner.strip_suffix(']')?)?;
        return Some(VariantSelector::Breakpoint { min_width: px });
    }
    if let Some(inner) = prefix.strip_prefix("max-[") {
        let px = parse_breakpoint_px(inner.strip_suffix(']')?)?;
        return Some(VariantSelector::MaxBreakpoint { max_width: px });
    }
    let rest = prefix.strip_prefix("data-[")?.strip_suffix(']')?;
    if let Some((k, v)) = rest.split_once('=') {
        Some(VariantSelector::Data {
            key: k.trim().to_string(),
            value: v.trim().trim_matches('"').trim_matches('\'').to_string(),
        })
    } else {
        Some(VariantSelector::Data {
            key: rest.trim().to_string(),
            value: "true".to_string(),
        })
    }
}

/// A single class token after stripping variant prefixes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassToken {
    pub variants: Vec<VariantSelector>,
    pub utility: String,
}

/// Parsed whitespace-separated class list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedClassList {
    pub base: Vec<String>,
    pub variants: Vec<(Vec<VariantSelector>, String)>,
}

/// Split `hover:dark:bg-primary` → variants + utility.
pub fn parse_class_token(token: &str) -> ClassToken {
    let mut variants = Vec::new();
    let mut rest = token;
    while let Some((prefix, after)) = rest.split_once(':') {
        // Arbitrary values like `bg-[#fff]` contain `:` inside brackets — stop if unbalanced.
        if prefix.contains('[') && !prefix.contains(']') {
            break;
        }
        // `data-[state=open]` is one prefix that itself contains `:`? No — uses `=`.
        // But `group-hover` etc. not yet supported.
        if let Some(sel) = VariantSelector::parse_prefix(prefix) {
            variants.push(sel);
            rest = after;
            continue;
        }
        // Unknown prefix — treat whole remainder including this segment as utility.
        break;
    }
    // If we broke out with an unknown prefix, restore full token as utility when no variants consumed.
    let utility = if variants.is_empty() && rest != token && token.contains(':') {
        // Had colons but first prefix wasn't a known variant — keep whole token.
        token.to_string()
    } else if !variants.is_empty() && !is_known_utility(rest) && rest.contains(':') {
        // e.g. hover:group-hover:bg-red — unknown; keep remainder as utility for warning
        rest.to_string()
    } else {
        rest.to_string()
    };
    ClassToken { variants, utility }
}

pub fn parse_class_list(classes: &str) -> ParsedClassList {
    let mut out = ParsedClassList::default();
    for raw in classes.split_whitespace() {
        let parsed = parse_class_token(raw);
        if parsed.variants.is_empty() {
            out.base.push(parsed.utility);
        } else {
            out.variants.push((parsed.variants, parsed.utility));
        }
    }
    out
}

#[cfg(test)]
#[path = "tests/variant.rs"]
mod tests;
