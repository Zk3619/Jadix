use super::*;
use gtk::accessible::{Property, Relation, State};
use jadix_core::presentation::accessibility::{CheckedState, LiveRegion, SemanticRole};

fn role(descriptor: &NodeDescriptor) -> gtk::AccessibleRole {
    use gtk::AccessibleRole as A;
    use SemanticRole::*;
    if descriptor.accessibility.accessible == Some(false) {
        return A::Presentation;
    }
    if descriptor.accessibility.role.is_none() {
        if let Some(role) = descriptor.accessible_role {
            use jadix_core::presentation::AccessibleRole as R;
            return match role {
                R::RadioGroup => A::RadioGroup,
                R::RadioItem { .. } => A::Radio,
                R::TabList => A::TabList,
                R::TabItem => A::Tab,
                R::TabPanel => A::TabPanel,
                R::Tree => A::Tree,
                R::TreeItem => A::TreeItem,
                R::ListBox => A::ListBox,
                R::ListBoxOption { .. } => A::Option,
                R::Heading => A::Heading,
                R::Grid { .. } => A::Grid,
                R::GridCell { .. } => A::GridCell,
                R::GridColumnHeader { .. } => A::ColumnHeader,
            };
        }
    }
    match descriptor.accessibility.role {
        Some(Button | Imagebutton | Key) => A::Button,
        Some(Link) => A::Link,
        Some(Textbox) => A::TextBox,
        Some(Img) => A::Img,
        Some(Text) => A::Label,
        Some(Adjustable) => A::Slider,
        Some(Heading) => A::Heading,
        Some(Summary) => A::Group,
        Some(Alert) => A::Alert,
        Some(Checkbox) => A::Checkbox,
        Some(Combobox) => A::ComboBox,
        Some(Menu) => A::Menu,
        Some(Menubar) => A::MenuBar,
        Some(Menuitem) => A::MenuItem,
        Some(Progressbar) => A::ProgressBar,
        Some(Radio) => A::Radio,
        Some(Radiogroup) => A::RadioGroup,
        Some(Scrollbar) => A::Scrollbar,
        Some(Spinbutton) => A::SpinButton,
        Some(Switch) => A::Switch,
        Some(Tab) => A::Tab,
        Some(Tablist) => A::TabList,
        Some(Timer) => A::Timer,
        Some(Toolbar) => A::Toolbar,
        Some(List) => A::List,
        Some(Grid) => A::Grid,
        Some(Table) => A::Table,
        Some(Row) => A::Row,
        Some(Cell) => A::Cell,
        Some(Columnheader) => A::ColumnHeader,
        Some(Rowgroup) => A::RowGroup,
        Some(Listbox) => A::ListBox,
        Some(Option) => A::Option,
        Some(Region) => A::Region,
        Some(Navigation) => A::Navigation,
        Some(Tree) => A::Tree,
        Some(Treeitem) => A::TreeItem,
        Some(Menuitemcheckbox) => A::MenuItemCheckbox,
        Some(Menuitemradio) => A::MenuItemRadio,
        Some(Dialog) => A::Dialog,
        Some(Alertdialog) => A::AlertDialog,
        None => match descriptor.content {
            NodeContent::Button { .. } => A::Button,
            NodeContent::Input { .. } | NodeContent::Textarea { .. } => A::TextBox,
            NodeContent::Checkbox { .. } => A::Checkbox,
            NodeContent::Switch { .. } => A::Switch,
            NodeContent::Select { .. } => A::ComboBox,
            NodeContent::Image { .. } => A::Img,
            NodeContent::Text(_) => A::Label,
            NodeContent::Dialog { .. } => A::Dialog,
            _ => A::Group,
        },
    }
}

pub(super) fn apply(item: &Item, descriptor: &NodeDescriptor) {
    let widget = &item.widget;
    let semantics = &descriptor.accessibility;
    widget.set_accessible_role(role(descriptor));
    for (property, value) in [
        (
            gtk::AccessibleProperty::Label,
            descriptor.accessible_name.as_deref(),
        ),
        (
            gtk::AccessibleProperty::Description,
            descriptor.accessible_description.as_deref(),
        ),
    ] {
        if let Some(value) = value {
            widget.update_property(&[if property == gtk::AccessibleProperty::Label {
                Property::Label(value)
            } else {
                Property::Description(value)
            }]);
        } else {
            widget.reset_property(property);
        }
    }
    widget.update_property(&[
        Property::ReadOnly(semantics.read_only),
        Property::MultiLine(matches!(descriptor.content, NodeContent::Textarea { .. })),
    ]);
    widget.update_state(&[
        State::Hidden(semantics.hidden),
        State::Disabled(semantics.disabled),
        State::Busy(semantics.busy),
        State::Invalid(if semantics.invalid {
            gtk::AccessibleInvalidState::True
        } else {
            gtk::AccessibleInvalidState::False
        }),
    ]);
    for (state, value) in [
        (
            gtk::AccessibleState::Selected,
            descriptor.accessible_selected,
        ),
        (
            gtk::AccessibleState::Expanded,
            descriptor.accessible_expanded,
        ),
    ] {
        if let Some(value) = value {
            widget.update_state(&[if state == gtk::AccessibleState::Selected {
                State::Selected(Some(value))
            } else {
                State::Expanded(Some(value))
            }]);
        } else {
            widget.reset_state(state);
        }
    }
    if let Some(checked) = semantics.checked {
        widget.update_state(&[State::Checked(match checked {
            CheckedState::False => gtk::AccessibleTristate::False,
            CheckedState::True => gtk::AccessibleTristate::True,
            CheckedState::Mixed => gtk::AccessibleTristate::Mixed,
        })]);
    } else if !matches!(
        descriptor.content,
        NodeContent::Checkbox { .. } | NodeContent::Switch { .. }
    ) {
        widget.reset_state(gtk::AccessibleState::Checked);
    }
    if let Some(pressed) = semantics.pressed {
        widget.update_state(&[State::Pressed(if pressed {
            gtk::AccessibleTristate::True
        } else {
            gtk::AccessibleTristate::False
        })]);
    } else {
        widget.reset_state(gtk::AccessibleState::Pressed);
    }
    if let Some(value) = &semantics.value {
        widget.update_property(&[
            Property::ValueMin(value.min),
            Property::ValueMax(value.max),
            Property::ValueNow(value.now),
            Property::ValueText(value.text.as_deref().unwrap_or("")),
        ]);
    } else {
        for property in [
            gtk::AccessibleProperty::ValueMin,
            gtk::AccessibleProperty::ValueMax,
            gtk::AccessibleProperty::ValueNow,
            gtk::AccessibleProperty::ValueText,
        ] {
            widget.reset_property(property);
        }
    }
    let previous = item.control.descriptor.borrow();
    if semantics.live != LiveRegion::Off && previous.content != descriptor.content {
        let text = descriptor
            .accessible_name
            .as_deref()
            .or(match &descriptor.content {
                NodeContent::Text(value) => Some(value.as_str()),
                _ => None,
            });
        if let Some(text) = text {
            widget.announce(
                text,
                if semantics.live == LiveRegion::Assertive {
                    gtk::AccessibleAnnouncementPriority::High
                } else {
                    gtk::AccessibleAnnouncementPriority::Medium
                },
            );
        }
    }
}

pub(super) fn actions(item: &Item, descriptor: &NodeDescriptor, events: Rc<Events>) {
    // GTK exposes parameterless widget actions through its AT-SPI Action bridge.
    // Admission is checked again at invocation against the current descriptor;
    // a stale action retained by an AT cannot activate a removed/disabled node.
    let group = gio::SimpleActionGroup::new();
    for advertised in &descriptor.accessibility.actions {
        let id = advertised.id.clone();
        let name = if gio::Action::name_is_valid(&id) {
            id.clone()
        } else {
            format!(
                "action-{}",
                id.as_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            )
        };
        let action =
            gio::SimpleAction::new(&name, (id == "setValue").then_some(glib::VariantTy::DOUBLE));
        action.set_enabled(if id == "setValue" {
            descriptor
                .accessibility
                .value
                .as_ref()
                .is_some_and(|range| descriptor.accepts_accessibility_action(&id, Some(range.now)))
        } else {
            descriptor.accepts_accessibility_action(&id, None)
        });
        let control = Rc::downgrade(&item.control);
        let queue = Rc::downgrade(&events);
        action.connect_activate(move |_, parameter| {
            if let (Some(control), Some(queue)) = (control.upgrade(), queue.upgrade()) {
                let value = parameter.and_then(|p| p.get::<f64>());
                if control
                    .descriptor
                    .borrow()
                    .accepts_accessibility_action(&id, value)
                {
                    queue.push(
                        &control,
                        EventPayload::AccessibilityAction {
                            id: id.clone(),
                            value,
                        },
                    );
                }
            }
        });
        group.add_action(&action);
    }
    item.widget.insert_action_group("jadix", Some(&group));
}

pub(super) fn relations(items: &BTreeMap<NodeId, Item>) {
    for item in items.values() {
        let descriptor = item.control.descriptor.borrow();
        if let Some(target) = descriptor
            .accessibility_relations
            .active_descendant
            .and_then(|id| items.get(&id))
        {
            item.widget
                .update_relation(&[Relation::ActiveDescendant(target.widget.upcast_ref())]);
        } else {
            item.widget
                .reset_relation(gtk::AccessibleRelation::ActiveDescendant);
        }
        let targets: Vec<&gtk::Accessible> = descriptor
            .accessibility_relations
            .controls
            .iter()
            .filter_map(|id| items.get(id))
            .map(|item| item.widget.upcast_ref())
            .collect();
        if targets.is_empty() {
            item.widget
                .reset_relation(gtk::AccessibleRelation::Controls);
        } else {
            item.widget.update_relation(&[Relation::Controls(&targets)]);
        }
    }
}
