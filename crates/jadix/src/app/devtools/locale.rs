use std::cell::RefCell;

thread_local! {
    static LAST: RefCell<String> = const { RefCell::new(String::new()) };
}

pub(super) fn sync() {
    let locale = crate::chrome_locale();
    LAST.with(|last| {
        if *last.borrow() != locale {
            *last.borrow_mut() = locale;
            super::DevToolsState::bump();
        }
    });
}

pub(super) fn text(en: &str) -> String {
    let zh = match en {
        "Sources is limited to Rust files inside the workspace." => "源代码编辑仅限工作区内的 Rust 文件。",
        "Source file exceeds the 512 KiB editor limit." => "源文件超过编辑器 512 KiB 的限制。",
        "File changed on disk. Copy your draft, then Reload from disk before saving." => "磁盘文件已更改。请保留草稿并重新加载后再保存。",
        "Enable source writes first." => "请先启用源码写入。",
        "Source path changed. Reload before saving." => "源文件路径已更改，请重新加载后再保存。",
        "Reload the source before saving." => "请先重新加载源文件。",
        "Reloaded source from disk; previous draft discarded." => "已从磁盘重新加载，原草稿已丢弃。",
        "no matching rules" => "没有匹配的样式规则",
        "Elements" => "元素",
        "Sources" => "源代码",
        "Selected element only; no global stylesheet changes." => "仅修改当前选中元素，不改变全局样式表。",
        "Declarations" => "声明文本",
        "Matched rules (read-only)" => "匹配规则（只读）",
        "Inherited text (read-only)" => "继承的文字样式（只读）",
        "Layout" => "布局",
        "Reactivity" => "响应式",
        "Changes" => "更改",
        "Element tree" => "元素树",
        "Select an element (Ctrl+Shift+C)" => "选择页面中的元素（Ctrl+Shift+C）",
        "Toggle device toolbar" => "切换设备工具栏",
        "Switch language" => "切换语言",
        "Narrow DevTools" => "缩小开发者工具",
        "Widen DevTools" => "放大开发者工具",
        "Detach DevTools" => "在独立窗口中打开",
        "Dock DevTools" => "停靠开发者工具",
        "Close DevTools" => "关闭开发者工具",
        "Properties" => "属性",
        "Styles" => "样式",
        "Computed" => "已计算",
        "Signal graph" => "信号图",
        "Find" => "查找",
        "Clear" => "清除",
        "Find tag, id, text, source" => "查找标签、标识、文本或源码",
        "No matches" => "无匹配元素",
        "Select a node or use Pick (Ctrl+Shift+C)." => "选择节点或使用元素拾取（Ctrl+Shift+C）。",
        "Select a node to inspect layout." => "选择节点以检查布局。",
        "Source" => "源文件",
        "Save policy" => "保存策略",
        "Hot reload" => "热重载",
        "Last result" => "最近结果",
        "Source unavailable" => "无法读取源文件",
        "Enable source writes" => "启用源码写入",
        "Source writes ON" => "源码写入已启用",
        "Save file" => "保存文件",
        "Reload from disk" => "从磁盘重新加载",
        "Clear history" => "清除历史",
        "Apply CSS" => "应用 CSS",
        "Stylesheet" => "样式表",
        "Scope" => "作用范围",
        "Box model" => "盒模型",
        "Pick an element to inspect its nearest ui! source." => "选择元素以检查最近的 ui! 源码。",
        "No source anchor on this node or its ancestors." => "此节点及其祖先没有源码锚点。",
        "Writes the full file after checking disk conflicts. Reload discards the draft." => "检查磁盘冲突后保存完整文件；重新加载将丢弃草稿。",
        "Start jadix_hot_reload_cli separately. ui! text/styles patch live; Rust logic needs rebuilding." => "运行热重载 CLI：静态 UI 文本和样式实时更新，Rust 逻辑由 --run 模式重新构建。",
        "CSS applies, source saves and hot-reload batches appear here. Latest 128 entries retained." => "显示样式应用、源码保存与热更新记录；保留最近 128 条。",
        "Apply CSS changes this session only. Sources saves real Rust code." => "应用 CSS 仅修改当前会话；源代码面板可保存真实 Rust 文件。",
        "F12 toggle  ·  Ctrl+Shift+C pick" => "F12 开关  ·  Ctrl+Shift+C 拾取",
        "HMR enabled  ·  F12 toggle  ·  Ctrl+Shift+C pick" => "热重载已启用  ·  F12 开关  ·  Ctrl+Shift+C 拾取",
        "Device preview" => "设备预览",
        "Responsive" => "自定义",
        "Desktop" => "桌面",
        "Tablet" => "平板",
        "Fit" => "适应",
        "Apply" => "应用",
        "Rotate" => "旋转",
        "Summary" => "概览",
        "Selected" => "已选节点",
        "Selected path" => "已选路径",
        "Selected focus_id" => "焦点标识",
        "Selected template_id" => "模板标识",
        "Filter terms" => "过滤条件",
        "Signal → Effect" => "信号 → 副作用",
        "Signal → Memo" => "信号 → 派生值",
        "Named signals" => "命名信号",
        "Source writes disabled" => "源码写入已禁用",
        _ => en,
    };
    crate::tr_default_zh(&format!("jadix.devtools.{en}"), en, zh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_translates_without_requiring_catalog_features() {
        let before = crate::chrome_locale();
        crate::set_chrome_locale("zh-CN");
        assert_eq!(text("Elements"), "元素");
        assert_eq!(text("Apply CSS"), "应用 CSS");
        crate::set_chrome_locale("en-US");
        assert_eq!(text("Elements"), "Elements");
        crate::set_chrome_locale(&before);
    }
}
