"""Regenerate the Gallery-only CJK subset; requires fonttools."""

from pathlib import Path
from fontTools import subset
from fontTools.ttLib import TTFont


def main():
    root = Path(__file__).resolve().parents[2]
    source = root / "crates/jadix/assets/NotoSansCJK.ttc"
    font = TTFont(source, fontNumber=27)
    text = "".join(
        path.read_text(encoding="utf-8")
        for directory in ("examples/gallery/src", "crates/jadix_components/src")
        for path in sorted((root / directory).rglob("*.rs"))
    )
    options = subset.Options()
    options.name_IDs = [0, 1, 2, 3, 4, 5, 6, 13, 14, 16, 17]
    options.name_legacy = True
    options.name_languages = ["*"]
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(text=text)
    subsetter.subset(font)
    for record in font["name"].names:
        if record.nameID in (1, 4, 6, 16):
            value = "JadixGallerySans" if record.nameID == 6 else "Jadix Gallery Sans"
            record.string = value.encode(record.getEncoding())
        elif record.nameID == 3:
            record.string = "Jadix Gallery Sans subset".encode(record.getEncoding())
    output = Path(__file__).with_name("assets") / "gallery-sans.otf"
    output.parent.mkdir(parents=True, exist_ok=True)
    font.save(output)
    print(output)


if __name__ == "__main__":
    main()
