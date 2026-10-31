"""Extract the complete Noto Sans CJK SC Regular face; requires fonttools."""

from pathlib import Path
from fontTools.ttLib import TTFont


def main():
    root = Path(__file__).resolve().parents[2]
    source = root / "crates/jadix/assets/NotoSansCJK.ttc"
    font = TTFont(source, fontNumber=27, recalcTimestamp=False)
    if font["name"].getDebugName(1) != "Noto Sans CJK SC" or font["name"].getDebugName(2) != "Regular":
        raise ValueError("Expected Noto Sans CJK SC Regular at collection index 27")
    output = Path(__file__).with_name("assets") / "gallery-sans.otf"
    output.parent.mkdir(parents=True, exist_ok=True)
    font.save(output)
    font.close()
    print(output)


if __name__ == "__main__":
    main()
