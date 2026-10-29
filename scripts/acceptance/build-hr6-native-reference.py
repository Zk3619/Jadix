import argparse
import difflib
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--evidence",type=Path,required=True)
    parser.add_argument("--patcher",type=Path,required=True)
    arguments=parser.parse_args()
    root=Path(__file__).resolve().parents[2]
    specification=importlib.util.spec_from_file_location("cost_builder",root/"scripts/acceptance/build-hr6-native-cost.py")
    helper=importlib.util.module_from_spec(specification)
    specification.loader.exec_module(helper)
    evidence=arguments.evidence.resolve();evidence.mkdir(exist_ok=False)
    folder=root/"crates/jadix/src/app/devtools"
    paths=[folder/name for name in ["native_acceptance_scene.rs","native_acceptance_state.rs","native_acceptance_matrix.rs"]]
    originals={path:path.read_bytes() for path in paths}
    before={path:content.decode("utf-8").replace("\r\n","\n") for path,content in originals.items()}
    after=dict(before)
    for path in paths[:2]:
        after[path]=before[path].replace("96.0","128.0").replace("HR6 A","HR6 B30")
    after[paths[2]]=before[paths[2]].replace("    register_description(compiled_builder_scene(),BODY)","    matrix_updated_scene()")
    assert all(after[path]!=before[path] for path in paths)
    changes={path:(before[path],after[path]) for path in paths}
    for path,content in originals.items():(evidence/path.name).write_bytes(content)
    patch="".join("".join(difflib.unified_diff(before[path].splitlines(keepends=True),after[path].splitlines(keepends=True),fromfile=path.as_posix(),tofile=path.as_posix())) for path in paths)
    (evidence/"actual-c-source.patch").write_text(patch,encoding="utf-8")
    records=[]
    try:
        helper.apply(arguments.patcher,changes)
        for variant,features in [("macros-both","macros,hot-reload,devtools,backend-dx12,images,a11y,animation-base"),("builder-both","hot-reload,devtools,backend-dx12,images,a11y,animation-base"),("core-only","hot-reload,backend-dx12,images,a11y,animation-base")]:
            command=["cargo","test","-p","jadix","--no-default-features","--features",features,"--lib","native_acceptance_tests","--no-run","--locked","--offline","--config","profile.test.package.jadix.opt-level=0","--config","profile.test.package.jadix.debug=0","--message-format=json"]
            log=evidence/f"{variant}-build.log"
            with log.open("w",encoding="utf-8") as output:subprocess.run(command,cwd=root,stdout=output,stderr=subprocess.STDOUT,check=True)
            artifacts=[]
            for line in log.read_text(encoding="utf-8").splitlines():
                try:item=json.loads(line)
                except ValueError:continue
                if item.get("reason")=="compiler-artifact" and item.get("executable") and item["target"]["name"]=="jadix":artifacts.append(item)
            retained=evidence/f"native-{variant}-c.exe";shutil.copy2(artifacts[-1]["executable"],retained)
            records.append({"variant":variant,"command":command,"sha256":hashlib.sha256(retained.read_bytes()).hexdigest(),"executable":str(retained),"compiled_source_sha256":{path.name:hashlib.sha256(path.read_bytes()).hexdigest() for path in paths},"artifact":artifacts[-1]})
            (evidence/"builds.json").write_text(json.dumps(records,indent=2),encoding="utf-8")
            print(variant,"C compiled",flush=True)
    finally:
        for path in paths:
            if path.read_text(encoding="utf-8")!=after[path]:raise RuntimeError(f"concurrent edit prevents restoration: {path}")
        helper.apply(arguments.patcher,{path:(after[path],before[path]) for path in paths})
        for path in paths:
            if path.read_bytes()!=originals[path]:shutil.copyfile(evidence/path.name,path)
        (evidence/"restoration.json").write_text(json.dumps({"all_a_sources_restored":all(path.read_bytes()==originals[path] for path in paths)},indent=2),encoding="utf-8")


if __name__=="__main__":
    main()
