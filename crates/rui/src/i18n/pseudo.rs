//! Accented / bidi pseudo-locales for catching missing translations.

/// `en-XA`: pad Latin letters with accents so untranslated English stands out.
pub fn accent_xa(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        out.push(match c {
            'a' => 'á',
            'e' => 'é',
            'i' => 'í',
            'o' => 'ø',
            'u' => 'ü',
            'A' => 'Å',
            'E' => 'É',
            'I' => 'Î',
            'O' => 'Ø',
            'U' => 'Ü',
            'c' => 'ç',
            'C' => 'Ç',
            'n' => 'ñ',
            'N' => 'Ñ',
            other => other,
        });
    }
    out
}

/// `en-XB`: wrap words so RTL bugs in mixed layouts are obvious.
pub fn wrap_xb(s: &str) -> String {
    format!("\u{202e}{s}\u{202c}")
}
