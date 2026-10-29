use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TemplateNode {
    Element {
        name: String,
        key: Option<String>,
        attributes: Vec<(String, String)>,
        children: Vec<TemplateNode>,
    },
    Text {
        value: String,
    },
    Slot {
        fingerprint: String,
    },
}

impl TemplateNode {
    pub fn same_shape(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Text { .. }, Self::Text { .. }) => true,
            (Self::Slot { fingerprint: left }, Self::Slot { fingerprint: right }) => left == right,
            (
                Self::Element {
                    name: left,
                    key: left_key,
                    children: left_children,
                    ..
                },
                Self::Element {
                    name: right,
                    key: right_key,
                    children: right_children,
                    ..
                },
            ) => {
                left == right
                    && left_key == right_key
                    && left_children.len() == right_children.len()
                    && left_children
                        .iter()
                        .zip(right_children)
                        .all(|(left, right)| left.same_shape(right))
            }
            _ => false,
        }
    }

    pub fn slots(&self, output: &mut Vec<String>) {
        match self {
            Self::Slot { fingerprint } => output.push(fingerprint.clone()),
            Self::Element { children, .. } => children.iter().for_each(|child| child.slots(output)),
            Self::Text { .. } => {}
        }
    }

    pub fn executable_signature(&self) -> Option<String> {
        let Self::Element { name, key, .. } = self else {
            return None;
        };
        let mut slots = Vec::new();
        self.slots(&mut slots);
        slots.sort();
        if slots.windows(2).any(|pair| pair[0] == pair[1]) {
            return None;
        }
        serde_json::to_string(&(name, key, slots)).ok()
    }
}
