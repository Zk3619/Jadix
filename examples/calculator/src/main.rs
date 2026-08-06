#![allow(non_snake_case)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rui::component;
use rui::components::*;
use rui::core::reactivity::{use_signal, Signal};
use rui::core::tree::Element;
use rui::style::parse_color;
use rui::ui;

#[derive(Clone, Copy, PartialEq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Mod,
}

#[derive(Clone, PartialEq)]
struct Engine {
    display: String,
    /// 结果行上方的表达式预览，例如 `123 + 4 =`。
    expr: String,
    acc: Option<f64>,
    pending: Option<Op>,
    fresh: bool,
    error: bool,
    memory: f64,
    deg: bool,
    second: bool,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            display: "0".into(),
            expr: String::new(),
            acc: None,
            pending: None,
            fresh: true,
            error: false,
            memory: 0.0,
            deg: true,
            second: false,
        }
    }
}

impl Engine {
    fn shown(&self) -> String {
        if self.error {
            return "错误".into();
        }
        format_number(parse_display(&self.display).unwrap_or(0.0))
    }

    fn preview(&self) -> String {
        if self.error {
            return String::new();
        }
        self.expr.clone()
    }

    fn clear_all(&mut self) {
        *self = Self {
            memory: self.memory,
            deg: self.deg,
            second: self.second,
            ..Self::default()
        };
    }

    fn clear_entry(&mut self) {
        self.display = "0".into();
        self.fresh = true;
        self.error = false;
    }

    fn backspace(&mut self) {
        if self.error || self.fresh {
            return;
        }
        let mut chars: Vec<char> = self.display.chars().collect();
        if chars.len() <= 1 || (chars.len() == 2 && chars[0] == '-') {
            self.display = "0".into();
            self.fresh = true;
            return;
        }
        chars.pop();
        self.display = chars.into_iter().collect();
    }

    fn input_digit(&mut self, d: char) {
        if self.error {
            self.clear_all();
        }
        if self.expr.ends_with('=') {
            self.expr.clear();
            self.acc = None;
        }
        if self.fresh {
            self.display = d.to_string();
            self.fresh = false;
            return;
        }
        if self.display == "0" && d != '.' {
            self.display = d.to_string();
            return;
        }
        if self.display == "-0" && d != '.' {
            self.display = format!("-{d}");
            return;
        }
        if self.display.chars().filter(|c| c.is_ascii_digit()).count() >= 16 {
            return;
        }
        self.display.push(d);
    }

    fn input_dot(&mut self) {
        if self.error {
            self.clear_all();
        }
        if self.fresh {
            self.display = "0.".into();
            self.fresh = false;
            return;
        }
        if !self.display.contains('.') {
            self.display.push('.');
        }
    }

    fn toggle_sign(&mut self) {
        if self.error {
            return;
        }
        if let Some(v) = parse_display(&self.display) {
            self.display = format_number(-v);
            self.fresh = false;
        }
    }

    fn apply_pending(&mut self) -> bool {
        let rhs = match parse_display(&self.display) {
            Some(v) => v,
            None => {
                self.error = true;
                return false;
            }
        };
        let Some(op) = self.pending else {
            self.acc = Some(rhs);
            return true;
        };
        let lhs = self.acc.unwrap_or(0.0);
        let out = match op {
            Op::Add => lhs + rhs,
            Op::Sub => lhs - rhs,
            Op::Mul => lhs * rhs,
            Op::Div => {
                if rhs == 0.0 {
                    self.error = true;
                    return false;
                }
                lhs / rhs
            }
            Op::Pow => lhs.powf(rhs),
            Op::Mod => {
                if rhs == 0.0 {
                    self.error = true;
                    return false;
                }
                lhs % rhs
            }
        };
        if !out.is_finite() {
            self.error = true;
            return false;
        }
        self.acc = Some(out);
        self.display = format_number(out);
        true
    }

    fn set_op(&mut self, op: Op) {
        if self.error {
            return;
        }
        if (!self.fresh || self.acc.is_none()) && !self.apply_pending() {
            return;
        }
        self.pending = Some(op);
        self.fresh = true;
        let lhs = self
            .acc
            .map(format_number)
            .unwrap_or_else(|| self.display.clone());
        self.expr = format!("{} {}", lhs, op_symbol(op));
    }

    fn equals(&mut self) {
        if self.error {
            return;
        }
        if let (Some(acc), Some(op)) = (self.acc, self.pending) {
            let rhs = self.display.clone();
            self.expr = format!("{} {} {} =", format_number(acc), op_symbol(op), rhs);
        }
        if self.pending.is_some() && !self.apply_pending() {
            self.expr.clear();
            return;
        }
        self.pending = None;
        self.fresh = true;
    }

    fn unary<F: FnOnce(f64) -> Option<f64>>(&mut self, f: F) {
        if self.error {
            return;
        }
        let Some(v) = parse_display(&self.display) else {
            self.error = true;
            return;
        };
        match f(v) {
            Some(out) if out.is_finite() => {
                self.display = format_number(out);
                self.fresh = true;
                self.expr.clear();
            }
            _ => self.error = true,
        }
    }

    fn percent(&mut self) {
        self.unary(|v| Some(v / 100.0));
    }
    fn reciprocal(&mut self) {
        self.unary(|v| if v == 0.0 { None } else { Some(1.0 / v) });
    }
    fn square(&mut self) {
        self.unary(|v| Some(v * v));
    }
    fn sqrt(&mut self) {
        self.unary(|v| if v < 0.0 { None } else { Some(v.sqrt()) });
    }
    fn abs(&mut self) {
        self.unary(|v| Some(v.abs()));
    }
    fn factorial(&mut self) {
        self.unary(|v| {
            if v < 0.0 || v.fract() != 0.0 || v > 170.0 {
                return None;
            }
            let n = v as u32;
            let mut acc = 1.0_f64;
            for i in 2..=n {
                acc *= i as f64;
            }
            Some(acc)
        });
    }
    fn exp10(&mut self) {
        self.unary(|v| Some(10.0_f64.powf(v)));
    }
    fn exp_e(&mut self) {
        self.unary(|v| Some(v.exp()));
    }
    fn log10(&mut self) {
        self.unary(|v| if v <= 0.0 { None } else { Some(v.log10()) });
    }
    fn ln(&mut self) {
        self.unary(|v| if v <= 0.0 { None } else { Some(v.ln()) });
    }
    fn sin(&mut self) {
        let deg = self.deg;
        self.unary(|v| Some(if deg { v.to_radians() } else { v }.sin()));
    }
    fn cos(&mut self) {
        let deg = self.deg;
        self.unary(|v| Some(if deg { v.to_radians() } else { v }.cos()));
    }
    fn tan(&mut self) {
        let deg = self.deg;
        self.unary(|v| Some(if deg { v.to_radians() } else { v }.tan()));
    }
    fn asin(&mut self) {
        let deg = self.deg;
        self.unary(|v| {
            if !(-1.0..=1.0).contains(&v) {
                return None;
            }
            let r = v.asin();
            Some(if deg { r.to_degrees() } else { r })
        });
    }
    fn acos(&mut self) {
        let deg = self.deg;
        self.unary(|v| {
            if !(-1.0..=1.0).contains(&v) {
                return None;
            }
            let r = v.acos();
            Some(if deg { r.to_degrees() } else { r })
        });
    }
    fn atan(&mut self) {
        let deg = self.deg;
        self.unary(|v| {
            let r = v.atan();
            Some(if deg { r.to_degrees() } else { r })
        });
    }
    fn const_pi(&mut self) {
        if self.error {
            self.clear_all();
        }
        self.display = format_number(std::f64::consts::PI);
        self.fresh = true;
    }
    fn const_e(&mut self) {
        if self.error {
            self.clear_all();
        }
        self.display = format_number(std::f64::consts::E);
        self.fresh = true;
    }
    fn mem_clear(&mut self) {
        self.memory = 0.0;
    }
    fn mem_recall(&mut self) {
        if self.error {
            self.clear_all();
        }
        self.display = format_number(self.memory);
        self.fresh = true;
    }
    fn mem_add(&mut self) {
        if let Some(v) = parse_display(&self.display) {
            self.memory += v;
            self.fresh = true;
        }
    }
    fn mem_sub(&mut self) {
        if let Some(v) = parse_display(&self.display) {
            self.memory -= v;
            self.fresh = true;
        }
    }
    fn mem_store(&mut self) {
        if let Some(v) = parse_display(&self.display) {
            self.memory = v;
            self.fresh = true;
        }
    }
}

fn parse_display(s: &str) -> Option<f64> {
    s.replace(',', "").parse().ok()
}

fn op_symbol(op: Op) -> &'static str {
    match op {
        Op::Add => "+",
        Op::Sub => "−",
        Op::Mul => "×",
        Op::Div => "÷",
        Op::Pow => "^",
        Op::Mod => "mod",
    }
}

fn format_number(v: f64) -> String {
    if !v.is_finite() {
        return "错误".into();
    }
    if v == 0.0 {
        return "0".into();
    }
    let abs = v.abs();
    if abs >= 1e16 || (abs < 1e-6 && abs > 0.0) {
        return format!("{v:.10e}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string();
    }
    let mut s = format!("{v:.12}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

fn rgba_u8(token: &str) -> [u8; 4] {
    let c = parse_color(token);
    [
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8,
        (c[3] * 255.0).round() as u8,
    ]
}

#[derive(Clone, Copy)]
enum KeyTone {
    Fn,
    Num,
    Eq,
}

fn mutate(eng: Signal<Engine>, mut f: impl FnMut(&mut Engine) + 'static) -> impl FnMut() + 'static {
    move || {
        eng.update(|e| f(e));
    }
}

fn calc_key(
    id: &str,
    caption: &str,
    tone: KeyTone,
    eng: Signal<Engine>,
    f: impl FnMut(&mut Engine) + 'static,
) -> Element {
    match tone {
        KeyTone::Fn => ui! {
            <Button
                id={id}
                variant="outline"
                w="100%"
                h="100%"
                min_h="3rem"
                min_w="0"
                rounded="0.25rem"
                border_style="solid"
                border_width=1.0
                border_color="#d4d4d4"
                bg="#f2f2f2"
                on_click={mutate(eng, f)}
            >
                { caption }
            </Button>
        },
        KeyTone::Num => ui! {
            <Button
                id={id}
                variant="secondary"
                w="100%"
                h="100%"
                min_h="3rem"
                min_w="0"
                rounded="0.25rem"
                border_style="solid"
                border_width=1.0
                border_color="#d4d4d4"
                bg="#ffffff"
                on_click={mutate(eng, f)}
            >
                { caption }
            </Button>
        },
        KeyTone::Eq => ui! {
            <Button
                id={id}
                variant="default"
                w="100%"
                h="100%"
                min_h="3rem"
                min_w="0"
                rounded="0.25rem"
                border_style="solid"
                border_width=1.0
                border_color="#0067c0"
                bg="#0067c0"
                on_click={mutate(eng, f)}
            >
                { caption }
            </Button>
        },
    }
}

fn mem_key(
    id: &str,
    caption: &str,
    enabled: bool,
    eng: Signal<Engine>,
    f: impl FnMut(&mut Engine) + 'static,
) -> Element {
    if enabled {
        ui! {
            <Button
                id={id}
                variant="ghost"
                grow="1"
                flex_basis="0"
                min_w="0"
                h="2rem"
                rounded="0.2rem"
                on_click={mutate(eng, f)}
            >
                { caption }
            </Button>
        }
    } else {
        ui! {
            <Button
                id={id}
                variant="ghost"
                disabled=true
                grow="1"
                flex_basis="0"
                min_w="0"
                h="2rem"
                rounded="0.2rem"
            >
                { caption }
            </Button>
        }
    }
}

fn key_flex_row(children: Vec<Element>) -> Element {
    let cells: Vec<Element> = children
        .into_iter()
        .map(|c| {
            ui! {
                <Rect grow="1" flex_basis="0" min_w="0" h="100%">
                    { c }
                </Rect>
            }
        })
        .collect();
    ui! {
        <HStack gap="0.25rem" w="100%" grow="1">
            { cells }
        </HStack>
    }
}

fn apply_key(eng: Signal<Engine>, key: &winit::event::KeyEvent) -> bool {
    use winit::event::ElementState;
    use winit::keyboard::{Key, NamedKey};

    if key.state != ElementState::Pressed {
        return false;
    }

    match &key.logical_key {
        Key::Character(ch) => {
            let s = ch.as_str();
            match s {
                "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                    let d = s.chars().next().unwrap();
                    mutate(eng, move |e| e.input_digit(d))();
                    true
                }
                "." | "," => {
                    mutate(eng, |e| e.input_dot())();
                    true
                }
                "+" => {
                    mutate(eng, |e| e.set_op(Op::Add))();
                    true
                }
                "-" => {
                    mutate(eng, |e| e.set_op(Op::Sub))();
                    true
                }
                "*" | "x" | "X" => {
                    mutate(eng, |e| e.set_op(Op::Mul))();
                    true
                }
                "/" => {
                    mutate(eng, |e| e.set_op(Op::Div))();
                    true
                }
                "%" => {
                    mutate(eng, |e| e.percent())();
                    true
                }
                "=" => {
                    mutate(eng, |e| e.equals())();
                    true
                }
                _ => false,
            }
        }
        Key::Named(NamedKey::Enter) => {
            mutate(eng, |e| e.equals())();
            true
        }
        Key::Named(NamedKey::Backspace) => {
            mutate(eng, |e| e.backspace())();
            true
        }
        Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Escape) => {
            mutate(eng, |e| e.clear_all())();
            true
        }
        _ => false,
    }
}

fn standard_pad(eng: Signal<Engine>) -> Element {
    ui! {
        <GStack cols="4" rows="6" gap="0.25rem" w="100%" h="100%">
            { calc_key("k-%", "%", KeyTone::Fn, eng, |e| e.percent()) }
            { calc_key("k-ce", "CE", KeyTone::Fn, eng, |e| e.clear_entry()) }
            { calc_key("k-c", "C", KeyTone::Fn, eng, |e| e.clear_all()) }
            { calc_key("k-bs", "⌫", KeyTone::Fn, eng, |e| e.backspace()) }
            { calc_key("k-inv", "1/x", KeyTone::Fn, eng, |e| e.reciprocal()) }
            { calc_key("k-sq", "x²", KeyTone::Fn, eng, |e| e.square()) }
            { calc_key("k-sqrt", "√x", KeyTone::Fn, eng, |e| e.sqrt()) }
            { calc_key("k-div", "÷", KeyTone::Fn, eng, |e| e.set_op(Op::Div)) }
            { calc_key("k-7", "7", KeyTone::Num, eng, |e| e.input_digit('7')) }
            { calc_key("k-8", "8", KeyTone::Num, eng, |e| e.input_digit('8')) }
            { calc_key("k-9", "9", KeyTone::Num, eng, |e| e.input_digit('9')) }
            { calc_key("k-mul", "×", KeyTone::Fn, eng, |e| e.set_op(Op::Mul)) }
            { calc_key("k-4", "4", KeyTone::Num, eng, |e| e.input_digit('4')) }
            { calc_key("k-5", "5", KeyTone::Num, eng, |e| e.input_digit('5')) }
            { calc_key("k-6", "6", KeyTone::Num, eng, |e| e.input_digit('6')) }
            { calc_key("k-sub", "−", KeyTone::Fn, eng, |e| e.set_op(Op::Sub)) }
            { calc_key("k-1", "1", KeyTone::Num, eng, |e| e.input_digit('1')) }
            { calc_key("k-2", "2", KeyTone::Num, eng, |e| e.input_digit('2')) }
            { calc_key("k-3", "3", KeyTone::Num, eng, |e| e.input_digit('3')) }
            { calc_key("k-add", "+", KeyTone::Fn, eng, |e| e.set_op(Op::Add)) }
            { calc_key("k-sign", "+/−", KeyTone::Num, eng, |e| e.toggle_sign()) }
            { calc_key("k-0", "0", KeyTone::Num, eng, |e| e.input_digit('0')) }
            { calc_key("k-dot", ".", KeyTone::Num, eng, |e| e.input_dot()) }
            { calc_key("k-eq", "=", KeyTone::Eq, eng, |e| e.equals()) }
        </GStack>
    }
}

fn scientific_pad(eng: Signal<Engine>) -> Element {
    let snap = eng.get();
    let sin_l = if snap.second { "sin⁻¹" } else { "sin" };
    let cos_l = if snap.second { "cos⁻¹" } else { "cos" };
    let tan_l = if snap.second { "tan⁻¹" } else { "tan" };
    let deg_l = if snap.deg { "DEG" } else { "RAD" };

    ui! {
        <VStack gap="0.25rem" w="100%" h="100%" grow="1">
            { key_flex_row(vec![
                calc_key("s-deg", deg_l, KeyTone::Fn, eng, |e| e.deg = !e.deg),
                calc_key("s-sin", sin_l, KeyTone::Fn, eng, |e| if e.second { e.asin() } else { e.sin() }),
                calc_key("s-cos", cos_l, KeyTone::Fn, eng, |e| if e.second { e.acos() } else { e.cos() }),
                calc_key("s-tan", tan_l, KeyTone::Fn, eng, |e| if e.second { e.atan() } else { e.tan() }),
            ]) }
            { key_flex_row(vec![
                calc_key("s-2nd", "2ⁿᵈ", KeyTone::Fn, eng, |e| e.second = !e.second),
                calc_key("s-pi", "π", KeyTone::Fn, eng, |e| e.const_pi()),
                calc_key("s-e", "e", KeyTone::Fn, eng, |e| e.const_e()),
                calc_key("s-c", "C", KeyTone::Fn, eng, |e| e.clear_all()),
                calc_key("s-bs", "⌫", KeyTone::Fn, eng, |e| e.backspace()),
            ]) }
            { key_flex_row(vec![
                calc_key("s-sq", "x²", KeyTone::Fn, eng, |e| e.square()),
                calc_key("s-inv", "1/x", KeyTone::Fn, eng, |e| e.reciprocal()),
                calc_key("s-abs", "|x|", KeyTone::Fn, eng, |e| e.abs()),
                calc_key("s-exp", "exp", KeyTone::Fn, eng, |e| e.exp_e()),
                calc_key("s-mod", "mod", KeyTone::Fn, eng, |e| e.set_op(Op::Mod)),
            ]) }
            { key_flex_row(vec![
                calc_key("s-sqrt", "√x", KeyTone::Fn, eng, |e| e.sqrt()),
                calc_key("s-lp", "(", KeyTone::Fn, eng, |_| {}),
                calc_key("s-rp", ")", KeyTone::Fn, eng, |_| {}),
                calc_key("s-fac", "n!", KeyTone::Fn, eng, |e| e.factorial()),
                calc_key("s-div", "÷", KeyTone::Fn, eng, |e| e.set_op(Op::Div)),
            ]) }
            { key_flex_row(vec![
                calc_key("s-pow", "xʸ", KeyTone::Fn, eng, |e| e.set_op(Op::Pow)),
                calc_key("s-7", "7", KeyTone::Num, eng, |e| e.input_digit('7')),
                calc_key("s-8", "8", KeyTone::Num, eng, |e| e.input_digit('8')),
                calc_key("s-9", "9", KeyTone::Num, eng, |e| e.input_digit('9')),
                calc_key("s-mul", "×", KeyTone::Fn, eng, |e| e.set_op(Op::Mul)),
            ]) }
            { key_flex_row(vec![
                calc_key("s-10x", "10ˣ", KeyTone::Fn, eng, |e| e.exp10()),
                calc_key("s-4", "4", KeyTone::Num, eng, |e| e.input_digit('4')),
                calc_key("s-5", "5", KeyTone::Num, eng, |e| e.input_digit('5')),
                calc_key("s-6", "6", KeyTone::Num, eng, |e| e.input_digit('6')),
                calc_key("s-sub", "−", KeyTone::Fn, eng, |e| e.set_op(Op::Sub)),
            ]) }
            { key_flex_row(vec![
                calc_key("s-log", "log", KeyTone::Fn, eng, |e| e.log10()),
                calc_key("s-1", "1", KeyTone::Num, eng, |e| e.input_digit('1')),
                calc_key("s-2", "2", KeyTone::Num, eng, |e| e.input_digit('2')),
                calc_key("s-3", "3", KeyTone::Num, eng, |e| e.input_digit('3')),
                calc_key("s-add", "+", KeyTone::Fn, eng, |e| e.set_op(Op::Add)),
            ]) }
            { key_flex_row(vec![
                calc_key("s-ln", "ln", KeyTone::Fn, eng, |e| e.ln()),
                calc_key("s-sign", "+/−", KeyTone::Num, eng, |e| e.toggle_sign()),
                calc_key("s-0", "0", KeyTone::Num, eng, |e| e.input_digit('0')),
                calc_key("s-dot", ".", KeyTone::Num, eng, |e| e.input_dot()),
                calc_key("s-eq", "=", KeyTone::Eq, eng, |e| e.equals()),
            ]) }
        </VStack>
    }
}

#[component]
fn CalculatorApp() -> Element {
    let mode = use_signal("calc_mode_str", || "standard".to_string());
    let eng = use_signal("calc_engine", Engine::default);

    rui::app::window_keys::set_window_key_handler(move |key| apply_key(eng, key));

    // Keep Select outside the engine-tracking subtree so digit clicks do not remount it.
    let mode_header = ui! {
        <HStack w="100%" items="center" gap="0.5rem" h="2.5rem">
            <Select
                id="calc:mode"
                value={mode}
                default_value="standard"
                options={vec![
                    ("standard".to_string(), "标准".to_string()),
                    ("scientific".to_string(), "科学".to_string()),
                ]}
                placeholder="模式"
                w="7rem"
            />
        </HStack>
    };

    let body = rui::reactive!("examples/calculator/body", move || {
        let e = eng.get();
        let has_mem = e.memory != 0.0;
        let display = e.shown();
        let preview = e.preview();
        let is_standard = mode.get() == "standard";

        let preview_el = ui! {
            <Label
                id="calc-preview"
                text={preview}
                text_size="0.9375rem"
                font_weight="400"
                color={rgba_u8("#6b6b6b")}
                w="100%"
                text_align="right"
            />
        };

        let display_el = ui! {
            <Label
                id="calc-display"
                text={display}
                text_size="2.75rem"
                font_weight="600"
                color={rgba_u8("#1a1a1a")}
                w="100%"
                text_align="right"
            />
        };

        let pad = if is_standard {
            standard_pad(eng)
        } else {
            scientific_pad(eng)
        };

        ui! {
            <VStack w="100%" h="100%" gap="0.35rem">
                <VStack w="100%" justify="end" items="stretch" px="0.4rem" pb="0.25rem" min_h="5.5rem" gap="0.15rem">
                    { preview_el }
                    { display_el }
                </VStack>

                <HStack w="100%" gap="0.15rem" mb="0.15rem">
                    { mem_key("mem-mc", "MC", has_mem, eng, |e| e.mem_clear()) }
                    { mem_key("mem-mr", "MR", has_mem, eng, |e| e.mem_recall()) }
                    { mem_key("mem-mp", "M+", true, eng, |e| e.mem_add()) }
                    { mem_key("mem-mm", "M−", true, eng, |e| e.mem_sub()) }
                    { mem_key("mem-ms", "MS", true, eng, |e| e.mem_store()) }
                </HStack>

                <Rect w="100%" grow="1" min_h="18rem">
                    { pad }
                </Rect>
            </VStack>
        }
    });

    ui! {
        <VStack w="100%" h="100%" bg="#f3f3f3" gap="0.35rem" p="0.6rem">
            { mode_header }
            { body }
        </VStack>
    }
}

fn main() {
    std::env::set_var("RUI_WINDOW_TITLE", "计算器");
    std::env::set_var("RUI_WINDOW_WIDTH", "380");
    std::env::set_var("RUI_WINDOW_HEIGHT", "620");
    // Calculator scene has very limited emoji usage; keep cache ceilings low to reduce working set.
    std::env::set_var("RUI_EMOJI_RASTER_CACHE_MAX", "16");
    std::env::set_var("RUI_EMOJI_NODE_CACHE_MAX", "8");

    rui::app::run(|| {
        ui! {
            <VStack w="100%" h="100%">
                <CalculatorApp />
            </VStack>
        }
    });
}
