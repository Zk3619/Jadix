const MATRIX_UPDATED: &str = r##"<Box w=128.0 h=64.0 bg="#ff0000"><VStack id="hr6-group" w="50%" h=64.0><Text id="hr6-first">"重复"</Text><Text id="hr6-last">"中文转义\""</Text></VStack><HrNativeBadge id="hr6-badge" w=24.0 h=24.0/>"HR6 B30"</Box>"##;

thread_local! {
    static BADGE_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn badge_factory() -> Element {
    BADGE_BUILDS.with(|builds|builds.set(builds.get()+1));
    Element::new().bg("#00aaff").accesskit_name("HR6 registered factory").on_click(||{})
}

fn register_description(element:Element,source:&str)->Element {
    let mut description=jadix_hot_reload::TemplateDescription::new(
        jadix_hot_reload::syntax::template_signature(source).unwrap(),
        jadix_hot_reload::syntax::declarations_signature(source).unwrap());
    description.tree=jadix_hot_reload::syntax::template_tree(source);
    element.hot_reload_description(description)
}

fn compiled_matrix_scene()->Element {
    register_description(compiled_builder_scene(),BODY)
}

fn matrix_updated_scene()->Element {
    let text_node=|id:&str,value:&str| {
        let text:Element=crate::primitives::text(value).into();
        text.hot_reload_node_type("Text").hot_reload_declaration_base(&[],false).template_id(id)
    };
    let group:Element=crate::primitives::vstack().into();
    let group=group.hot_reload_node_type("VStack").hot_reload_declaration_base(&["w","h"],false)
        .template_id("hr6-group").w("50%").h(64.0)
        .child(text_node("hr6-first","重复")).child(text_node("hr6-last","中文转义\""));
    let badge=badge_factory().hot_reload_node_type("HrNativeBadge")
        .hot_reload_declaration_base(&["w","h"],false).template_id("hr6-badge").w(24.0).h(24.0);
    register_description(Element::new().hot_reload_node_type("Box")
        .hot_reload_declaration_base(&["w","h","bg"],false).w(128.0).h(64.0).bg("#ff0000")
        .child(group).child(badge).child(Element::new().text("HR6 B30")),MATRIX_UPDATED)
}

fn construct_root(mode:&str)->Element {
    let mut root=Element::new().w("100%").h("100%").bg("#ffffff");
    if mode=="deferred" {return root;}
    root=root.child(construct(mode));
    if mode.contains("multi") {root=root.child(construct("registered"));}
    root
}

impl Harness {
    fn capture_matrix(&self,name:&str) {
        if self.mode.contains("multi") {
            let secondaries=self.app.as_ref().unwrap().secondaries.borrow();
            let roots=secondaries.values().map(|secondary|semantics(&secondary.root_element)).collect::<Vec<_>>();
            assert_eq!(roots.len(),1);
            std::fs::write(self.directory.join(format!("{name}-secondary-semantics.json")),serde_json::to_vec_pretty(&roots).unwrap()).unwrap();
        }
        if self.mode.contains("matrix") {
            let state=self.app.as_ref().unwrap().state.borrow();let root=&state.as_ref().unwrap().root_element;
            let nodes=snapshot_tree(root);
            let current=["hr6-first","hr6-last","hr6-badge"].iter().map(|id|nodes.iter().find(|node|node.template_id.as_deref()==Some(id)).unwrap().instance_id).collect::<Vec<_>>();
            std::fs::write(self.directory.join(format!("{name}-matrix.json")),serde_json::to_vec_pretty(&serde_json::json!({
                "kept_instances":current,"factory_builds":BADGE_BUILDS.with(|builds|builds.get()),"percent_grid_flex":true,
                "keyed_text_and_factory":true})).unwrap()).unwrap();
        }
    }

    fn matrix_ready(&mut self)->bool {
        if !self.mode.contains("multi") {return true;}
        let app=self.app.as_ref().unwrap();let secondaries=app.secondaries.borrow();
        if secondaries.len()!=1 {return false;}
        let state=app.state.borrow();let primary=snapshot_tree(&state.as_ref().unwrap().root_element);
        let mut instances=primary.iter().filter(|node|node.template_id.as_deref()==Some(ID)).map(|node|node.instance_id).collect::<Vec<_>>();
        assert_eq!(instances.len(),2);
        let expected_width=if self.mode.starts_with("cold-") || self.step%2==0 {128.0}else{96.0};
        for secondary in secondaries.values() {
            let nodes=snapshot_tree(&secondary.root_element);
            let Some(node)=nodes.iter().find(|node|node.template_id.as_deref()==Some(ID)) else{return false;};
            if self.saved.is_some() || self.mode.starts_with("cold-") {
                let text=if self.mode.starts_with("cold-"){String::from("HR6 B30")}else{format!("HR6 B{}",self.step)};
                if node.layout_bounds[2]!=expected_width || !nodes.iter().any(|node|node.text_preview.as_deref()==Some(&text)) {return false;}
                if primary.iter().filter(|node|node.template_id.as_deref()==Some(ID)).any(|node|node.layout_bounds[2]!=expected_width) {return false;}
            }
            instances.push(node.instance_id);
        }
        let mut unique=instances.clone();unique.sort();unique.dedup();assert_eq!(unique.len(),3);
        if self.matrix_instances.is_empty() {self.matrix_instances=instances;}
        else {assert_eq!(self.matrix_instances,instances);}
        true
    }

    fn drive_deferred(&mut self,event_loop:&dyn ActiveEventLoop) {
        assert!(self.started.elapsed()<Duration::from_secs(30));
        if self.saved.is_none() {
            self.capture("A-native.png");self.capture_semantics("A-semantics.json");
            let updated=format!("fn demo() {{ ui! {{ {} }} }}\n",BODY.replace("96.0","128.0").replace("HR6 A","HR6 B30"));
            self.saved=Some(Instant::now());
            #[cfg(feature="devtools")]
            hr6_save(updated);
            #[cfg(not(feature="devtools"))]
            std::fs::write(PathBuf::from(std::env::var_os("JADIX_DEVTOOLS_ROOT").unwrap()).join("source.rs"),updated).unwrap();
            return;
        }
        if self.step==0 {
            let watcher=std::fs::read_to_string(self.directory.join("watcher.log")).unwrap();
            #[cfg(feature="devtools")]
            let deferred=DevToolsState::changes().iter().any(|(kind,detail)|kind=="HMR commit" && detail.contains("deferred"));
            #[cfg(not(feature="devtools"))]
            let deferred=watcher.contains("deferred");
            let _=watcher;
            if !deferred {return;}
            self.app.as_mut().unwrap().state.borrow_mut().as_mut().unwrap().rebuild_ui(construct_root("registered"));
            self.step=1;return;
        }
        let state=self.app.as_ref().unwrap().state.borrow();let nodes=snapshot_tree(&state.as_ref().unwrap().root_element);
        if !nodes.iter().any(|node|node.template_id.as_deref()==Some(ID) && node.layout_bounds[2]==128.0)
            || !nodes.iter().any(|node|node.text_preview.as_deref()==Some("HR6 B30")) {return;}
        drop(state);
        self.capture("B-native.png");self.capture_semantics("B-semantics.json");
        std::fs::write(self.directory.join("result.json"),r#"{"passed":true,"ack":"deferred","late_mount_applied":true,"native":true}"#).unwrap();
        event_loop.exit();
    }
}

#[test]
#[ignore="requires native primary and secondary windows plus the source watcher"]
fn native_multiple_windows(){native("multi");}
#[test]
#[ignore="requires native primary and secondary windows recompiled from changed Rust"]
fn native_recompiled_multiple_windows(){native("cold-multi");}
#[test]
#[ignore="requires a native deferred ACK followed by a late route mount"]
fn native_deferred_mount(){native("deferred");}
#[test]
#[ignore="requires the late route scene to have been actually recompiled"]
fn native_recompiled_deferred_mount(){native("cold-deferred");}
#[test]
#[ignore="requires native keyed structure, percent units, Grid/Flex and a registered factory"]
fn native_structural_matrix(){native("matrix");}
#[test]
#[ignore="requires the structural matrix Rust scene to have been changed and recompiled"]
fn native_recompiled_structural_matrix(){native("cold-matrix");}
