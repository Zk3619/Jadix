import argparse
import collections
import json
import re
from pathlib import Path


def read_tests(path):
    text = path.read_text(encoding="utf-8-sig", errors="replace")
    return re.findall(r"^test (\S+) \.\.\. (ok|FAILED|ignored)", text, re.MULTILINE)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--logs", type=Path, nargs="+", required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    baseline = dict(read_tests(arguments.baseline))
    if not baseline:
        raise SystemExit("Baseline has no executed test identities")
    observations = collections.defaultdict(list)
    for path in arguments.logs:
        for name, status in read_tests(path):
            observations[name].append({"log": str(path), "status": status})
    renames = {
        "platform::os::windows::ime::tests::lone_shift_with_compstr_requests_compose_finish":
            "platform::os::windows::ime::tests::only_lone_shift_release_finishes_composition",
        "utils::icon_morph::tests::menu_name_is_not_path_d": "menu_name_is_not_path_d",
        "utils::icon_morph::tests::unlock_aliases_lock_open_paths": "unlock_aliases_lock_open_paths",
        "utils::icon_morph::tests::lucide_menu_to_x_mid": "lucide_menu_to_x_mid",
        "components::button_group::tests::hovered_joined_segment_covers_adjacent_seam":
            "components::button_group::tests::hovered_joined_segment_preserves_adjacent_seam",
        "text::document::tests::indexes_empty_trailing_crlf_and_unicode_lines":
            "document::tests::indexes_empty_trailing_crlf_and_unicode_lines",
        "text::document::tests::replacement_preserves_snapshot_and_checks_byte_boundaries":
            "document::tests::replacement_preserves_snapshot_and_checks_byte_boundaries",
    }
    moved_modules = {
        "app::glass_params::": "glass_params::",
        "core::reactivity::": "reactivity::",
        "core::store::": "store::",
        "layout::dimension::": "dimension::",
        "layout::grid::": "grid::",
        "platform::os::windows::backdrop::": "backdrop::",
        "renderer::backdrop_blur::": "backdrop_blur::",
        "renderer::compute::": "compute::",
        "renderer::emoji_config::": "emoji_config::",
        "renderer::emoji_detect::": "emoji_detect::",
        "renderer::emoji_matrix::": "emoji_matrix::",
        "renderer::extended_pictographic::": "extended_pictographic::",
        "renderer::gpu_caps::": "gpu_caps::",
        "renderer::twemoji_name::": "twemoji_name::",
        "style::color::": "color::",
        "style::color_space::": "color_space::",
        "style::easing::": "easing::",
        "style::generator::": "generator::",
        "style::theme::": "theme::",
        "style::theme_override::": "theme_override::",
        "style::timeline::": "timeline::",
        "style::timing::": "timing::",
        "utils::icon_morph::": "icon_morph::",
    }
    records = []
    missing = []
    weakened = []
    for name, status in sorted(baseline.items()):
        current = renames.get(name, name)
        if current not in observations:
            for original, extracted in moved_modules.items():
                if current == name and name.startswith(original):
                    current = extracted + name[len(original):]
                    break
        matches = observations.get(current, [])
        if not matches:
            missing.append(name)
        elif status == "ok" and not any(item["status"] == "ok" for item in matches):
            weakened.append(name)
        records.append({"baseline": name, "baseline_status": status,
                        "current": current, "observations": matches})
    result = {"baseline": str(arguments.baseline), "baseline_count": len(baseline),
              "current_unique_count": len(observations), "missing": missing,
              "passed_to_nonpassing": weakened, "explicit_mappings": renames, "extracted_module_mappings": moved_modules,
              "records": records}
    arguments.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({key: value for key, value in result.items() if key != "records"}, indent=2))
    if missing or weakened:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
