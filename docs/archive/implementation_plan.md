> **已冻结归档**（2026-08-09）。非权威；说明见 [README.md](README.md)。

# Root Cause & Unification Proposal

I have successfully traced the root cause of the "Dubai" font appearing in your terminal logs, and I have a clear plan to achieve 100% consistent fonts across all platforms (Windows, macOS, Linux).

## Why "Dubai" is showing up (The Root Cause)
1. **The ShadCN Token:** In the UI, components like `Label` use `font-medium` (which corresponds to weight `500`).
2. **Segoe UI Limitations:** On Windows, the "Segoe UI" font family lacks a native `500` (Medium) weight. It only has Regular (400), Semibold (600), and Bold (700).
3. **The Fallback Mechanism:** When `cosmic-text` requests "Segoe UI" at weight `500`, it fails to find an exact match inside the Segoe UI family. Because it strictly wants a `500` weight, it searches the rest of the Windows system fonts. It finds "Dubai", which *does* have a `500` weight, and switches to it.

**In browsers**, the CSS engine would just use "Segoe UI 400" or synthetically embolden it. Our Rust text renderer (`cosmic-text`) switches the font family entirely to satisfy the weight requirement.

## ShadCN on the Web
> "ShadCN在不同平台的Web都使用segoe ui吗？" 

**No.** While ShadCN CSS includes `Segoe UI` in its fallback stack, **99% of ShadCN projects (like Next.js templates) bundle the "Inter" font by default via Google Fonts.** 
This means on the web, ShadCN uses "Inter" regardless of whether the user is on Windows or macOS.

## User Review Required

> [!TIP]
> **Proposal for 100% Unification:**
> To solve the weight issues and make the RUI framework look exactly like ShadCN on the Web, I propose we **bundle the "Inter" font** into the framework.

### Proposed Changes

#### 1. Download & Bundle "Inter"
- Download the `Inter-Variable.ttf` (which contains ALL weights from 100 to 900 smoothly).
- Place it in the `assets/` directory next to `lucide.ttf`.

#### 2. Load the Font at Runtime
- In `crates/rui/src/renderer/text.rs`, load the Inter font bytes directly into the `fontdb` memory during initialization.
```rust
db.load_font_data(include_bytes!("../../assets/Inter-Variable.ttf").to_vec());
```

#### 3. Set as Default
- Set "Inter" as the primary fallback and default `sans-serif` font for the framework.

**Benefits:**
- Your UI will perfectly render `font-medium` (`w=500`) without jumping to "Dubai".
- The UI will look 100% identical on Windows, macOS, and Linux.
- Text will perfectly match the standard ShadCN web aesthetic.

Please let me know if you approve this approach! Once approved, I will download the Inter font and integrate it.
