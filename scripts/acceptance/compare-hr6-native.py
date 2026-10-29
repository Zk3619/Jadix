import argparse
import hashlib
import json
from pathlib import Path
import re
from PIL import Image, ImageChops


def load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def accesskit(path):
    identities={}
    def replace(match):
        return "#"+str(identities.setdefault(match.group(),len(identities)))
    return re.sub(r"#\d+",replace,path.read_text(encoding="utf-8"))


def pixels(path):
    image=Image.open(path).convert("RGBA")
    device=load(path.parent/"device.json")
    if device.get("capture_encoding")!="rgba8":
        assert device["surface_format"] in ["Bgra8UnormSrgb","Bgra8Unorm"]
        blue,green,red,alpha=image.split()
        image=Image.merge("RGBA",(red,green,blue,alpha))
    derived=path.with_name(path.stem+"-rgba.png")
    image.save(derived)
    return image.convert("RGB")


def compare(root, live, cold):
    before=root/live;after=root/cold
    files=[before/"B-native.png",after/"C-native.png",before/"B-semantics.json",after/"C-semantics.json",before/"B-accesskit.json",after/"C-accesskit.json",before/"device.json",after/"device.json"]
    live_pixels=pixels(files[0]);cold_pixels=pixels(files[1])
    assert live_pixels.size==cold_pixels.size
    result={"live":live,"cold":cold,"pixel_bounds":ImageChops.difference(live_pixels,cold_pixels).getbbox(),
            "semantic_tree_equal":load(files[2])==load(files[3]),"accesskit_tree_equal":accesskit(files[4])==accesskit(files[5]),
            "device_equal":load(files[6])==load(files[7])}
    original=before/"A-native.png"
    assert ImageChops.difference(pixels(original),live_pixels).getbbox() is not None,"positive control must actually change native pixels"
    for suffix in ["secondary-semantics.json","events.json"]:
        left=before/("B-"+suffix);right=after/("C-"+suffix)
        if left.exists() or right.exists():
            first=load(left);second=load(right)
            if suffix=="events.json":
                assert first.pop("ime_commit_checked") and not second.pop("ime_commit_checked")
            result[suffix+"_equal"]=first==second
            files.extend([left,right])
    result["sources"]={str(path.relative_to(root)):hashlib.sha256(path.read_bytes()).hexdigest() for path in files+[original]}
    result["passed"]=result["pixel_bounds"] is None and all(value for key,value in result.items() if key.endswith("equal"))
    return result


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--evidence",type=Path,required=True)
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--prefix",default="native-settled")
    parser.add_argument("--cold-prefix",default="native-verified")
    arguments=parser.parse_args();root=arguments.evidence.resolve()
    prefix=arguments.prefix
    cold_prefix=arguments.cold_prefix
    pairs=[("static_ui_sources_watcher","recompiled_static"),("registered_builder_sources_watcher","recompiled_builder"),("stateful_static","recompiled_stateful_static"),("stateful_registered","recompiled_stateful_registered"),("multiple_windows","recompiled_multiple_windows"),("deferred_mount","recompiled_deferred_mount"),("structural_matrix","recompiled_structural_matrix")]
    results=[compare(root,prefix+"-macros/"+live,cold_prefix+"-c-macros/"+cold) for live,cold in pairs]
    for variant in ["builder","core-only"]:
        for live,cold in [("registered_builder_sources_watcher","recompiled_builder"),("stateful_registered","recompiled_stateful_registered")]:
            results.append(compare(root,f"{prefix}-{variant}/"+live,f"{cold_prefix}-c-{variant}/"+cold))
    equivalent=[]
    for variant,live,registered in [(variant,live,registered) for variant in ["builder","core-only"] for live,registered in [("static_ui_sources_watcher","registered_builder_sources_watcher"),("stateful_static","stateful_registered")]]:
        first=root/(prefix+"-macros")/live;second=root/(prefix+"-"+variant)/registered
        equivalent.append({"scenes":[str(first.relative_to(root)),str(second.relative_to(root))],
            "pixels_equal":ImageChops.difference(pixels(first/"B-native.png"),pixels(second/"B-native.png")).getbbox() is None,
            "semantic_equal":load(first/"B-semantics.json")==load(second/"B-semantics.json"),
            "accesskit_equal":accesskit(first/"B-accesskit.json")==accesskit(second/"B-accesskit.json")})
    passed=all(row["passed"] for row in results) and all(all(value for key,value in row.items() if key.endswith("equal")) for row in equivalent)
    arguments.output.write_text(json.dumps({"passed":passed,"comparisons":results,"macro_builder_equivalence":equivalent,
        "accesskit_normalization":"only process-local #NodeId values renumbered by first reference; complete ordered Debug tree retained",
        "pixels":"exact RGB equality over every physical pixel; no tolerated region or average-error threshold",
        "capture_decoding":"legacy unmarked PNGs contain retained BGRA bytes; lossless R/B swap based on recorded texture format, originals preserved, derived -rgba.png files; new capture helper writes RGBA directly",
        "state":"live input histories verified separately; cold recompilation does not reproduce editing history"},indent=2),encoding="utf-8")
    print(json.dumps({"passed":passed,"pairs":len(results),"macro_builder_pairs":len(equivalent)}))
    raise SystemExit(0 if passed else 1)


if __name__=="__main__":
    main()
