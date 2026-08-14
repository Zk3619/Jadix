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
    /// Responsive `sm:` / `md:` / … — matches when viewport width ≥ `min_width`.
    Breakpoint {
        min_width: u32,
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
            "group-hover" => Some(Self::GroupHover { name: None }),
            "peer-hover" => Some(Self::PeerHover { name: None }),
            _ => {
                if let Some(name) = prefix.strip_prefix("group-hover/") {
                    let name = name.trim();
                    if !name.is_empty() {
                        return Some(Self::GroupHover {
                            name: Some(name.to_string()),
                        });
                    }
                }
                if let Some(name) = prefix.strip_prefix("peer-hover/") {
                    let name = name.trim();
                    if !name.is_empty() {
                        return Some(Self::PeerHover {
                            name: Some(name.to_string()),
                        });
                    }
                }
                // data-[state=open] or data-[open] (boolean attr style → value "true")
                let rest = prefix.strip_prefix("data-[")?.strip_suffix(']')?;
                if let Some((k, v)) = rest.split_once('=') {
                    Some(Self::Data {
                        key: k.trim().to_string(),
                        value: v.trim().trim_matches('"').trim_matches('\'').to_string(),
                    })
                } else {
                    Some(Self::Data {
                        key: rest.trim().to_string(),
                        value: "true".to_string(),
                    })
                }
            }
        }
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
mod tests {
    use super::*;

    #[test]
    fn hover_bg() {
        let t = parse_class_token("hover:bg-accent");
        assert_eq!(t.variants, vec![VariantSelector::Hover]);
        assert_eq!(t.utility, "bg-accent");
    }

    #[test]
    fn stacked_dark_hover() {
        let t = parse_class_token("dark:hover:bg-primary");
        assert_eq!(
            t.variants,
            vec![VariantSelector::Dark, VariantSelector::Hover]
        );
        assert_eq!(t.utility, "bg-primary");
    }

    #[test]
    fn data_state() {
        let t = parse_class_token("data-[state=open]:bg-accent");
        assert_eq!(
            t.variants,
            vec![VariantSelector::Data {
                key: "state".into(),
                value: "open".into()
            }]
        );
        assert_eq!(t.utility, "bg-accent");
    }

    #[test]
    fn arbitrary_keeps_colon() {
        let t = parse_class_token("bg-[#ff00aa]");
        assert!(t.variants.is_empty());
        assert_eq!(t.utility, "bg-[#ff00aa]");
    }

    #[test]
    fn breakpoint_sm_bg() {
        let t = parse_class_token("sm:bg-red");
        assert_eq!(
            t.variants,
            vec![VariantSelector::Breakpoint {
                min_width: BREAKPOINT_SM
            }]
        );
        assert_eq!(t.utility, "bg-red");
    }

    #[test]
    fn breakpoint_2xl_stacked() {
        let t = parse_class_token("md:hover:p-4");
        assert_eq!(
            t.variants,
            vec![
                VariantSelector::Breakpoint {
                    min_width: BREAKPOINT_MD
                },
                VariantSelector::Hover
            ]
        );
        assert_eq!(t.utility, "p-4");
    }

    #[test]
    fn group_hover_and_named() {
        let t = parse_class_token("group-hover:bg-accent");
        assert_eq!(t.variants, vec![VariantSelector::GroupHover { name: None }]);
        assert_eq!(t.utility, "bg-accent");
        let named = parse_class_token("group-hover/card:opacity-50");
        assert_eq!(
            named.variants,
            vec![VariantSelector::GroupHover {
                name: Some("card".into())
            }]
        );
        assert_eq!(named.utility, "opacity-50");
    }

    #[test]
    fn peer_hover_and_named() {
        let t = parse_class_token("peer-hover:underline");
        assert_eq!(t.variants, vec![VariantSelector::PeerHover { name: None }]);
        assert_eq!(t.utility, "underline");
        let named = parse_class_token("peer-hover/input:border-primary");
        assert_eq!(
            named.variants,
            vec![VariantSelector::PeerHover {
                name: Some("input".into())
            }]
        );
        assert_eq!(named.utility, "border-primary");
    }
}
