import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def apply(patcher, changes):
    patch = "*** Begin Patch\n"
    for path, (before, after) in changes.items():
        differences = list(difflib.unified_diff(before.splitlines(), after.splitlines(), lineterm=""))[2:]
        patch += "*** Update File: " + path.as_posix() + "\n"
        patch += "\n".join("@@" if line.startswith("@@") else line for line in differences) + "\n"
    patch += "*** End Patch\n"
    subprocess.run([str(patcher), "--codex-run-as-apply-patch", patch], check=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--target", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--patcher", type=Path, required=True)
    parser.add_argument("--variants", nargs="+", choices=["base", "devfeatures"], default=["base"])
    parser.add_argument("--text-input", action="store_true")
    arguments = parser.parse_args()
    root = arguments.root.resolve(strict=True)
    evidence = arguments.evidence.resolve()
    evidence.mkdir(exist_ok=False)
    harness_relative = "crates/jadix/src/app/tests/native_cost_tests.rs" if (root / "crates/jadix/src/app/tests/native_cost_tests.rs").exists() else "crates/jadix/src/app/native_cost_tests.rs"
    relatives = ["Cargo.toml", "crates/jadix/Cargo.toml", "crates/jadix/src/lib.rs", "crates/jadix/src/app.rs", harness_relative]
    originals = {root / relative:(root / relative).read_bytes() for relative in relatives}
    before = {path:content.decode("utf-8").replace("\r\n","\n") for path,content in originals.items()}
    after = dict(before)
    manifest = root / "crates/jadix/Cargo.toml"
    after[manifest] += '\n[[bin]]\nname = "hr6_native_release_cost"\npath = "src/lib.rs"\n'
    library = root / "crates/jadix/src/lib.rs"
    after[library] += '\n#[cfg(jadix_hr6_native_cost)]\nfn main() {\n    app::run_native_release_cost();\n    println!("HR6_NATIVE_COST_COMPLETE");\n}\n'
    application = root / "crates/jadix/src/app.rs"
    gate = '#[cfg(all(test, windows))]\n'
    if harness_relative.endswith('/tests/native_cost_tests.rs'):
        gate += '#[path = "app/tests/native_cost_tests.rs"]\n'
    gate += 'mod native_cost_tests;'
    after[application] = after[application].replace(gate, gate.replace('#[cfg(all(test, windows))]', '#[cfg(all(windows, any(test, jadix_hr6_native_cost)))]') + '\n#[cfg(jadix_hr6_native_cost)]\npub(crate) fn run_native_release_cost() { native_cost_tests::native_release_cost(); }')
    assert after[application] != before[application], "production entry gate must be installed"
    harness = root / harness_relative
    position = after[harness].index("#[test]\n#[ignore =")
    body = after[harness].index("fn native_release_cost()", position)
    after[harness] = after[harness][:position] + "pub(super) " + after[harness][body:]
    if arguments.text_input:
        field = r'''
fn cost_input() -> Element {
    use crate::utils::text_field::*;
    use crate::utils::text_edit::{build_text_edit_element,TextEditBuildParams};
    crate::core::reactivity::use_signal(&input_value_signal_key("hr6-cost-input"),||"seed000".to_string());
    build_text_edit_element(TextEditBuildParams {
        field_id:"hr6-cost-input".into(),placeholder:"HR6 native input".into(),disabled:false,invalid:false,
        input_type:crate::utils::text_edit::field::InputType::Text,clearable:false,password_toggle:false,
        number_steppers:false,step:1.0,text_size_px:14.0,font_weight:400,line_height_px:None,leading_scale:None,
        value:None,text_align:crate::style::TextAlign::Start,single_line_container:Some(crate::primitives::hstack()),
        multiline:false,rows:1,resizable:false,resize_max_h:0.0,wrap_mode:TextareaWrapMode::Word,
        plain_chrome:false,caret_color:None,selection_bg:None,selection_foreground:None,
        selection_on_blur:SelectionOnBlur::Retain,undo:crate::utils::text_edit_history::TextUndoConfig::input_default(),drop_undo_on_unmount:true,
    }).w(260.0).h(36.0)
}
'''
        text_event = r'''
                use crate::core::reactivity::use_signal;
                use crate::utils::text_field::*;
                let field="hr6-cost-input";
                let value=use_signal(&input_value_signal_key(field),String::new);
                let prefix=format!("seed{:03}",self.frame%1000);
                value.set_silent(prefix.clone());
                use_signal(&input_caret_signal_key(field),||None::<usize>).set_silent(Some(prefix.len()));
                use_signal(&input_selection_signal_key(field),||None::<(usize,usize)>).set_silent(None);
                use_signal(&input_preedit_signal_key(field),String::new).set_silent(String::new());
                use_signal("__jadix_button_focus_id",String::new).set(format!("input:{field}"));
                app.about_to_wait(event_loop);
                app.window_event(event_loop,window,WindowEvent::Ime(winit::event::Ime::Enabled));
                app.window_event(event_loop,window,WindowEvent::Ime(winit::event::Ime::Preedit("中".into(),Some((3,3)))));
                app.window_event(event_loop,window,WindowEvent::Ime(winit::event::Ime::Commit("中".into())));
                assert_eq!(value.peek(),format!("{prefix}中"),"each measured input frame must commit through the native text controller");
'''
        after[harness]=field+after[harness]
        after[harness]=after[harness].replace('Element::new().w(640.0).h(480.0).bg("#ffffff").child(viewport)','Element::new().w(640.0).h(480.0).flex_col().bg("#ffffff").child(cost_input()).child(viewport)')
        after[harness]=after[harness].replace('.h(460.0).translate(','.h(440.0).translate(')
        after[harness]=after[harness].replace('(80.0 + (self.frame % 100) as f64, 80.0)','(80.0 + (self.frame % 100) as f64, 20.0)')
        after[harness]=after[harness].replace('"pointer_input"','"pointer_text_ime_input"').replace('"schema": 2,','"schema": 2,"verified_text_input":true,"verified_text_input_frames":self.samples[1].len(),')
        anchor='            }\n            2 => {'
        assert anchor in after[harness]
        after[harness]=after[harness].replace(anchor,text_event+anchor,1)
    changes = {path:(before[path],content) for path,content in after.items() if content != before[path]}
    records = []
    for path,content in originals.items():
        backup = evidence / "originals" / path.relative_to(root)
        backup.parent.mkdir(parents=True,exist_ok=True)
        backup.write_bytes(content)
    overlay = "".join("".join(difflib.unified_diff(before[path].splitlines(keepends=True),after[path].splitlines(keepends=True),fromfile=path.as_posix(),tofile=path.as_posix())) for path in changes)
    (evidence / "profiling-overlay.patch").write_text(overlay,encoding="utf-8")
    try:
        apply(arguments.patcher,changes)
        for variant in arguments.variants:
            features = "backend-dx12" + (",devtools,hot-reload" if variant == "devfeatures" else "")
            command = ["cargo","rustc","-p","jadix","--release","--no-default-features","--features",features,"--bin","hr6_native_release_cost","--locked","--offline","--target-dir",str(arguments.target),"--message-format=json","--","--cfg","jadix_hr6_native_cost","--check-cfg","cfg(jadix_hr6_native_cost)"]
            log = evidence / f"{variant}-build.log"
            with log.open("w",encoding="utf-8") as output:
                subprocess.run(command,cwd=root,env=dict(os.environ,CARGO_INCREMENTAL="0"),stdout=output,stderr=subprocess.STDOUT,check=True)
            executable = arguments.target / "release/hr6_native_release_cost.exe"
            retained = evidence / f"native-production-{variant}.exe"
            shutil.copy2(executable,retained)
            records.append({"variant":variant,"command":command,"executable":str(retained),"sha256":hashlib.sha256(retained.read_bytes()).hexdigest(),"bytes":retained.stat().st_size,"cfg_test":False,"debug_assertions":False,"panic":"abort"})
            (evidence / "builds.json").write_text(json.dumps(records,indent=2),encoding="utf-8")
            print(f"production {variant} built",flush=True)
    finally:
        for path in changes:
            if path.read_text(encoding="utf-8") != after[path].replace("\r\n","\n"):
                raise RuntimeError(f"concurrent edit prevents automatic restoration: {path}")
        apply(arguments.patcher,{path:(after[path],before[path]) for path in changes})
        for path in changes:
            if path.read_bytes()!=originals[path]:
                shutil.copyfile(evidence / "originals" / path.relative_to(root),path)
        (evidence / "restoration.json").write_text(json.dumps({"all_originals_restored":all(path.read_bytes()==content for path,content in originals.items()),"files":{str(path.relative_to(root)):hashlib.sha256(content).hexdigest() for path,content in originals.items()}},indent=2),encoding="utf-8")


if __name__ == "__main__":
    main()
