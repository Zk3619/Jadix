impl Harness {
    fn schedule_sample(&mut self,event_loop:&dyn ActiveEventLoop,name:&str) {
        if self.mode.contains("stateful") {
            self.finish_state(event_loop,name);
            self.pending_sample=Some((name.into(),Instant::now()));
        }else {self.capture_result(event_loop,name);}
    }

    fn drive_sample(&mut self,event_loop:&dyn ActiveEventLoop) {
        let name={
        let (name,started)=self.pending_sample.as_ref().unwrap();
        assert!(started.elapsed()<Duration::from_secs(5),"deterministic native sample did not settle");
        if started.elapsed()<Duration::from_millis(200) {return;}
        let state=self.app.as_ref().unwrap().state.borrow();
        let state=state.as_ref().unwrap();
        let root=&state.root_element;
        fn contains(element:&Element,text:&str)->bool {
            element.text.as_deref()==Some(text)||element.children.iter().any(|child|contains(child,text))
        }
        if !contains(root,"seed中文") {return;}
        for index in 0..6 {
            let Some(row)=super::pointer_events::find_by_focus_id(root,&format!("hr6-windowed-{index}")) else{return;};
            if row.layout_bounds[2]<=0.0 || row.layout_bounds[3]<=0.0 || row.text_style.font_family.is_none() {return;}
        }
        name.clone()
        };
        self.pending_sample=None;
        self.capture_result(event_loop,&name);
    }

    fn capture_result(&mut self,event_loop:&dyn ActiveEventLoop,name:&str) {
        self.capture_matrix(name);
        self.capture(&format!("{name}-native.png"));
        self.capture_semantics(&format!("{name}-semantics.json"));
        let result=if name=="B" {
            let mut sorted = self.samples.clone();
            sorted.sort_by(f64::total_cmp);
            serde_json::json!({"passed":true,"mode":self.mode,"samples":self.samples,"median_ms":sorted[sorted.len()/2],"p95_ms":sorted[((sorted.len() as f64*0.95).ceil() as usize).saturating_sub(1)],"watcher_debounce_ms":400,"instance":self.instance,"pid":std::process::id(),"elapsed_seconds":self.started.elapsed().as_secs_f64(),"private_mib":self.memory_samples,"registrations":format!("{:?}",self.registry_baseline),"scope":"real native Sources save/watcher/TCP/present; stable instance and registrations; memory samples; C requires separate actual compilation"})
        }else {serde_json::json!({"passed":true,"scope":"cold native scene from modified, recompiled Rust fixture"})};
        std::fs::write(self.directory.join("result.json"),serde_json::to_vec_pretty(&result).unwrap()).unwrap();
        self.finish_or_wait(event_loop);
    }
}
