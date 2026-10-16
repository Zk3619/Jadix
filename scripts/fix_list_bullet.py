import pathlib

p = pathlib.Path(__file__).resolve().parents[1] / "crates/jadix/src/components/list.rs"
lines = p.read_text(encoding="utf-8", errors="replace").splitlines(keepends=True)
bullet = "\u2022"

for i, line in enumerate(lines):
    if "Self::Disc => Some(" in line:
        lines[i] = f'            Self::Disc => Some("{bullet}"),\n'
    if "fn list_item_sets_listitem_role" in line:
        # fix assertion a few lines below
        for j in range(i, min(i + 20, len(lines))):
            if "c.text.as_deref()" in lines[j]:
                lines[j] = (
                    f'        assert!(el.children.iter().any(|c| c.text.as_deref() == Some("{bullet}")));\n'
                )
                break

p.write_text("".join(lines), encoding="utf-8")
print("fixed", p)
