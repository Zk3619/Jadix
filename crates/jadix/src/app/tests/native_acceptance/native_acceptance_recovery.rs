impl Harness {
    fn drive_recovery(&mut self,event_loop:&dyn ActiveEventLoop) {
        assert!(self.started.elapsed()<Duration::from_secs(40),"native negative ACK timeout");
        if self.started.elapsed()<Duration::from_secs(3) {return;}
        let cases=[
            BODY.replace("w=96.0","hr6_unknown=96.0"),
            BODY.replace("Box","HrUnknownFactory"),
            BODY.replace("\"HR6 A\"","\"HR6 A\" {new_rust_capture()}"),
            BODY.replace("96.0","128.0").replace("HR6 A","HR6 B30"),
        ];
        let updated=format!("fn demo() {{ ui! {{ {} }} }}\n",cases[self.step]);
        if self.saved.is_none() {
            if self.step==0 {
                let guard=self.app.as_ref().unwrap().state.borrow();
                self.state_expected=Some(semantics(&guard.as_ref().unwrap().root_element));
                drop(guard);
                self.capture("A-native.png");self.capture_semantics("A-semantics.json");
            }
            DevToolsState::clear_changes();
            self.recovery_errors=std::fs::read_to_string(self.directory.join("watcher.log")).unwrap().matches("hmr error").count();
            self.saved=Some(Instant::now());hr6_save(updated);
            return;
        }
        let changes=DevToolsState::changes();
        let acknowledgement=changes.iter().rev().find(|(kind,_)|kind=="HMR commit").map(|(_,detail)|serde_json::from_str::<jadix_hot_reload::CommitAck>(detail).unwrap());
        let watcher=std::fs::read_to_string(self.directory.join("watcher.log")).unwrap();
        let diagnostics=watcher.lines().filter(|line|line.contains("hmr error")).collect::<Vec<_>>();
        if acknowledgement.is_none() && diagnostics.len()<=self.recovery_errors {return;}
        let observation=if let Some(ack)=&acknowledgement {serde_json::json!({"stage":"runtime","acknowledgement":ack})}
            else {serde_json::json!({"stage":"watcher","acknowledged":false,"rebuild_reason":diagnostics.last().unwrap()})};
        assert_eq!(hr6_draft(),updated,"Inspector document must retain rejected source draft");
        let root=self.app.as_ref().unwrap().state.borrow();
        let root=&root.as_ref().unwrap().root_element;
        if self.step<3 {
            if let Some(ack)=&acknowledgement {
                assert!(matches!(ack.status,jadix_hot_reload::CommitStatus::NeedsRebuild|jadix_hot_reload::CommitStatus::Rejected));
                assert!(!ack.reason.is_empty());
            }else {assert!(diagnostics.last().unwrap().contains("rebuild"));}
            assert_eq!(Some(&semantics(root)),self.state_expected.as_ref(),"failed transaction must retain native UI");
        }else {
            assert_eq!(acknowledgement.as_ref().unwrap().status,jadix_hot_reload::CommitStatus::Applied);
            let nodes=snapshot_tree(root);
            if !nodes.iter().any(|node|node.text_preview.as_deref()==Some("HR6 B30")) || !nodes.iter().any(|node|node.template_id.as_deref()==Some(ID)&&node.layout_bounds[2]==128.0) {return;}
        }
        std::fs::write(self.directory.join(format!("ack-{}.json",self.step)),serde_json::to_vec_pretty(&observation).unwrap()).unwrap();
        std::fs::write(self.directory.join(format!("draft-{}.rs",self.step)),updated).unwrap();
        if self.step==3 {
            self.capture("B-native.png");self.capture_semantics("B-semantics.json");
            std::fs::write(self.directory.join("result.json"),br#"{"passed":true,"rejected_cases":3,"ui_and_draft_retained":true,"repair_applied":true}"#).unwrap();
            event_loop.exit();return;
        }
        self.step+=1;self.saved=None;
    }
}

#[test]
#[ignore="requires native Inspector Sources, watcher and rejected transaction recovery"]
fn native_rejected_draft_recovery(){native("recovery");}
