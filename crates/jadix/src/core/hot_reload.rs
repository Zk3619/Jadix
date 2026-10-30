use super::tree::Element;
use jadix_hot_reload::TemplateNode;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

type Factory = fn() -> Element;
thread_local! {
    static FACTORIES: RefCell<HashMap<String, (u32, Factory)>> = RefCell::new(HashMap::new());
}

pub fn register_factory(name: &str, version: u32, factory: Factory) -> Result<(), String> {
    if name.is_empty() || version == 0 || builtin(name).is_some() {
        return Err("invalid or reserved factory identity".into());
    }
    FACTORIES.with(|factories| {
        let mut factories = factories.borrow_mut();
        if let Some((previous, implementation)) = factories.get(name) {
            if *previous != version || !std::ptr::fn_addr_eq(*implementation, factory) {
                return Err("factory identity already registered".into());
            }
        } else {
            factories.insert(name.into(), (version, factory));
        }
        Ok(())
    })
}

fn builtin(name: &str) -> Option<Factory> {
    match name {
        "Box" => Some(|| crate::primitives::div().into()),
        "VStack" => Some(|| crate::primitives::vstack().into()),
        "HStack" => Some(|| crate::primitives::hstack().into()),
        "GStack" => Some(|| crate::primitives::gstack().into()),
        "Text" => Some(|| crate::primitives::text("").into()),
        _ => None,
    }
}

fn factory(name: &str) -> Option<(u32, Factory)> {
    builtin(name)
        .map(|factory| (1, factory))
        .or_else(|| FACTORIES.with(|factories| factories.borrow().get(name).copied()))
}

fn inline_text(tree: &TemplateNode) -> Option<&str> {
    match tree {
        TemplateNode::Element { name, children, .. } if name == "Text" => match children.as_slice()
        {
            [TemplateNode::Text { value }] => Some(value),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn bind(element: &mut Element, tree: &TemplateNode) -> bool {
    fn compatible(element: &Element, tree: &TemplateNode) -> bool {
        match tree {
            TemplateNode::Slot { .. } => true,
            TemplateNode::Text { .. } => element.text.is_some() && element.children.is_empty(),
            TemplateNode::Element { name, children, .. } => {
                factory(name).is_some()
                    && element.attrs.get("jadix_ui_node_type") == Some(name)
                    && if inline_text(tree).is_some() {
                        element.text.is_some() && element.children.is_empty()
                    } else {
                        element.children.len() == children.len()
                            && element
                                .children
                                .iter()
                                .zip(children)
                                .all(|(element, child)| compatible(element, child))
                    }
            }
        }
    }
    fn annotate(element: &mut Element, tree: &TemplateNode) {
        match tree {
            TemplateNode::Slot { fingerprint } => {
                element.set_attr("jadix_hr_slot", fingerprint);
            }
            TemplateNode::Text { .. } => {
                element.set_attr("jadix_hr_text", "1");
            }
            TemplateNode::Element {
                name,
                key,
                children,
                ..
            } => {
                element.set_attr(
                    "jadix_hr_factory_version",
                    factory(name).unwrap().0.to_string(),
                );
                if let Some(key) = key {
                    element.set_attr("jadix_hr_key", key);
                }
                if inline_text(tree).is_none() {
                    for (child, descriptor) in element.children.iter_mut().zip(children) {
                        annotate(child, descriptor);
                    }
                }
            }
        }
    }
    if tree.executable_signature().is_none() || !compatible(element, tree) {
        return false;
    }
    annotate(element, tree);
    if let Ok(description) = serde_json::to_string(tree) {
        element.set_attr("jadix_hr_tree", description);
    }
    true
}

pub(crate) fn preflight(root: &Element, tree: &TemplateNode) -> Result<(), String> {
    let old: TemplateNode = root
        .attrs
        .get("jadix_hr_tree")
        .and_then(|value| serde_json::from_str(value).ok())
        .ok_or("no compiled structural registration")?;
    if old.executable_signature().is_none()
        || old.executable_signature() != tree.executable_signature()
    {
        return Err("root, opaque slot, or capture contract changed; rebuild required".into());
    }
    let mut required = Vec::new();
    tree.slots(&mut required);
    let mut available = Vec::new();
    let mut versions = HashMap::new();
    fn inspect(element: &Element, slots: &mut Vec<String>, versions: &mut HashMap<String, String>) {
        if let Some(slot) = element.attrs.get("jadix_hr_slot") {
            slots.push(slot.clone());
            return;
        }
        if let (Some(name), Some(version)) = (
            element.attrs.get("jadix_ui_node_type"),
            element.attrs.get("jadix_hr_factory_version"),
        ) {
            versions.insert(name.clone(), version.clone());
        }
        for child in &element.children {
            inspect(child, slots, versions);
        }
    }
    inspect(root, &mut available, &mut versions);
    available.sort();
    required.sort();
    if available != required {
        return Err("compiled slot instance is unavailable or duplicated".into());
    }
    fn validate(
        tree: &TemplateNode,
        keys: &mut HashSet<String>,
        versions: &HashMap<String, String>,
        depth: usize,
        count: &mut usize,
    ) -> Result<(), String> {
        *count += 1;
        if depth > 128 || *count > 4096 {
            return Err("template structure budget exceeded".into());
        }
        if let TemplateNode::Element {
            name,
            key,
            attributes,
            children,
        } = tree
        {
            let (version, _) =
                factory(name).ok_or_else(|| format!("unregistered component factory: {name}"))?;
            if versions
                .get(name)
                .is_some_and(|compiled| compiled != &version.to_string())
            {
                return Err("component factory version changed".into());
            }
            if key
                .as_ref()
                .is_some_and(|key| key.is_empty() || !keys.insert(key.clone()))
            {
                return Err("empty or duplicate structural key".into());
            }
            if attributes
                .iter()
                .any(|(name, _)| !jadix_hot_reload::is_live_attribute(name))
            {
                return Err("unsupported structural attribute".into());
            }
            for child in children {
                validate(child, keys, versions, depth + 1, count)?;
            }
        }
        Ok(())
    }
    validate(tree, &mut HashSet::new(), &versions, 0, &mut 0)
}

pub(crate) struct PreparedTemplate {
    elements: Vec<Option<Element>>,
    data_only: bool,
}

pub(crate) fn prepare(root: &Element, tree: &TemplateNode) -> Result<PreparedTemplate, String> {
    preflight(root, tree)?;
    let old: TemplateNode = serde_json::from_str(root.attrs.get("jadix_hr_tree").unwrap())
        .map_err(|reason| reason.to_string())?;
    if old.same_shape(tree) {
        return Ok(PreparedTemplate {
            elements: Vec::new(),
            data_only: true,
        });
    }
    let mut keys = HashSet::new();
    fn inspect(element: &Element, keys: &mut HashSet<(String, String)>) {
        if element.attrs.contains_key("jadix_hr_slot") {
            return;
        }
        if let (Some(key), Some(name)) = (
            element.attrs.get("jadix_hr_key"),
            element.attrs.get("jadix_ui_node_type"),
        ) {
            keys.insert((key.clone(), name.clone()));
        }
        for child in &element.children {
            inspect(child, keys);
        }
    }
    inspect(root, &mut keys);
    let mut elements = Vec::new();
    fn build(
        tree: &TemplateNode,
        elements: &mut Vec<Option<Element>>,
        keys: &HashSet<(String, String)>,
        is_root: bool,
    ) -> Result<(), String> {
        if let TemplateNode::Element {
            name,
            key,
            children,
            ..
        } = tree
        {
            if is_root
                || key
                    .as_ref()
                    .is_some_and(|key| keys.contains(&(key.clone(), name.clone())))
            {
                elements.push(None);
            } else {
                let (_, factory) = factory(name).ok_or("factory disappeared")?;
                let mut element = factory();
                if !element.children.is_empty() {
                    return Err("factory must expose an empty authored children slot".into());
                }
                element.set_attr("jadix_ui_node_type", name);
                elements.push(Some(element.hot_reload_declaration_base(&[], false)));
            }
            for child in children {
                build(child, elements, keys, false)?;
            }
        }
        Ok(())
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build(tree, &mut elements, &keys, true)
    }))
    .map_err(|_| "component factory panicked before tree mutation")??;
    Ok(PreparedTemplate {
        elements,
        data_only: false,
    })
}

pub(crate) fn apply_prepared(
    root: &mut Element,
    tree: &TemplateNode,
    prepared: PreparedTemplate,
    mut declarations: impl FnMut(&mut Element, &[(String, String)]),
) {
    if prepared.data_only {
        fn data(
            element: &mut Element,
            tree: &TemplateNode,
            declarations: &mut impl FnMut(&mut Element, &[(String, String)]),
        ) {
            match tree {
                TemplateNode::Text { value } => {
                    element.text = Some(value.clone());
                }
                TemplateNode::Element {
                    attributes,
                    children,
                    ..
                } => {
                    declarations(element, attributes);
                    if let Some(value) = inline_text(tree) {
                        element.text = Some(value.to_string());
                    } else {
                        for (element, tree) in element.children.iter_mut().zip(children) {
                            data(element, tree, declarations);
                        }
                    }
                }
                TemplateNode::Slot { .. } => {}
            }
        }
        data(root, tree, &mut declarations);
        bind(root, tree);
        return;
    }
    let mut keyed = HashMap::new();
    let mut slots = HashMap::new();
    fn take(
        mut element: Element,
        keyed: &mut HashMap<(String, String), Element>,
        slots: &mut HashMap<String, Element>,
    ) {
        if let Some(slot) = element.attrs.get("jadix_hr_slot").cloned() {
            slots.insert(slot, element);
            return;
        }
        for child in std::mem::take(&mut element.children) {
            take(child, keyed, slots);
        }
        if let (Some(key), Some(name)) = (
            element.attrs.get("jadix_hr_key"),
            element.attrs.get("jadix_ui_node_type"),
        ) {
            keyed.insert((key.clone(), name.clone()), element);
        }
    }
    for child in std::mem::take(&mut root.children) {
        take(child, &mut keyed, &mut slots);
    }
    fn build(
        tree: &TemplateNode,
        prepared: &mut std::vec::IntoIter<Option<Element>>,
        keyed: &mut HashMap<(String, String), Element>,
        slots: &mut HashMap<String, Element>,
        declarations: &mut impl FnMut(&mut Element, &[(String, String)]),
    ) -> Element {
        match tree {
            TemplateNode::Slot { fingerprint } => {
                slots.remove(fingerprint).expect("preflight checked slot")
            }
            TemplateNode::Text { value } => Element::new().text(value.clone()),
            TemplateNode::Element {
                name,
                key,
                attributes,
                children,
            } => {
                let fresh = prepared.next().expect("prepared factory");
                let mut element = key
                    .as_ref()
                    .and_then(|key| keyed.remove(&(key.clone(), name.clone())))
                    .or(fresh)
                    .expect("prepared or existing keyed element");
                declarations(&mut element, attributes);
                if let Some(key) = key {
                    element.set_attr("jadix_hr_key", key);
                    if element.template_id.is_none() {
                        element.template_id = Some(key.clone());
                    }
                }
                if let Some(value) = inline_text(tree) {
                    element.text = Some(value.to_string());
                    element.children.clear();
                } else {
                    element.children = children
                        .iter()
                        .map(|child| build(child, prepared, keyed, slots, declarations))
                        .collect();
                }
                element
            }
        }
    }
    let mut prepared = prepared.elements.into_iter();
    let _ = prepared.next();
    if let TemplateNode::Element {
        attributes,
        children,
        ..
    } = tree
    {
        declarations(root, attributes);
        if let Some(value) = inline_text(tree) {
            root.text = Some(value.to_string());
            root.children.clear();
        } else {
            root.children = children
                .iter()
                .map(|child| {
                    build(
                        child,
                        &mut prepared,
                        &mut keyed,
                        &mut slots,
                        &mut declarations,
                    )
                })
                .collect();
        }
    }
    bind(root, tree);
}

#[cfg(any(all(debug_assertions, feature = "hot-reload"), feature = "devtools", test))]
pub(crate) fn instances(root: &Element) -> HashMap<u64, Vec<usize>> {
    let mut output = HashMap::new();
    let mut stack = vec![(root, Vec::new())];
    while let Some((element, path)) = stack.pop() {
        output.insert(element.inspector_instance_id, path.clone());
        for (index, child) in element.children.iter().enumerate() {
            let mut next = path.clone();
            next.push(index);
            stack.push((child, next));
        }
    }
    output
}

#[cfg(any(all(debug_assertions, feature = "hot-reload"), test))]
pub(crate) fn changes(
    before: &HashMap<u64, Vec<usize>>,
    root: &Element,
) -> Vec<jadix_hot_reload::NodeChange> {
    let after = instances(root);
    let mut identities: Vec<_> = before.keys().chain(after.keys()).copied().collect();
    identities.sort();
    identities.dedup();
    identities
        .into_iter()
        .map(|instance| jadix_hot_reload::NodeChange {
            instance,
            before: before.get(&instance).cloned(),
            after: after.get(&instance).cloned(),
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/hot_reload.rs"]
mod tests;

pub(crate) fn impact(root: &Element, tree: &TemplateNode) -> crate::core::tree::LayoutStatus {
    use crate::core::tree::LayoutStatus;
    fn changed(old: &TemplateNode, new: &TemplateNode) -> LayoutStatus {
        match (old, new) {
            (TemplateNode::Text { value: old }, TemplateNode::Text { value: new }) => {
                if old == new {
                    LayoutStatus::Clean
                } else {
                    LayoutStatus::LayoutChanged
                }
            }
            (
                TemplateNode::Element {
                    attributes: old,
                    children: old_children,
                    ..
                },
                TemplateNode::Element {
                    attributes: new,
                    children: new_children,
                    ..
                },
            ) => {
                let mut impact = LayoutStatus::Clean;
                for (name, value) in old.iter().chain(new) {
                    if old.iter().find(|(candidate, _)| candidate == name)
                        == new.iter().find(|(candidate, _)| candidate == name)
                    {
                        continue;
                    }
                    let _ = value;
                    impact = impact.merge(match jadix_hot_reload::attribute_impact(name) {
                        Some(jadix_hot_reload::AttributeImpact::Paint) => {
                            LayoutStatus::StyleChanged
                        }
                        _ => LayoutStatus::LayoutChanged,
                    });
                }
                old_children
                    .iter()
                    .zip(new_children)
                    .fold(impact, |impact, (old, new)| impact.merge(changed(old, new)))
            }
            _ => LayoutStatus::Clean,
        }
    }
    let old: Option<TemplateNode> = root
        .attrs
        .get("jadix_hr_tree")
        .and_then(|value| serde_json::from_str(value).ok());
    match old {
        Some(old) if old.same_shape(tree) => changed(&old, tree),
        _ => LayoutStatus::StructureChanged,
    }
}
