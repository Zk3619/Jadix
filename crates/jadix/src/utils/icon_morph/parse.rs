//! SVG path `d` parser. Relative commands and H/V/S/T shorthands are resolved
//! to absolute L/C/Q/A; conversion to cubics lives in `normalize`.

use super::types::{morph_err, MorphError};

pub(crate) enum RawSeg {
    L(f64, f64),
    C(f64, f64, f64, f64, f64, f64),
    Q(f64, f64, f64, f64),
    A(f64, f64, f64, u8, u8, f64, f64),
}

pub(crate) struct RawSubpath {
    pub x0: f64,
    pub y0: f64,
    pub segs: Vec<RawSeg>,
    pub closed: bool,
}

fn is_command(c: u8) -> bool {
    matches!(
        c,
        b'M' | b'm'
            | b'L'
            | b'l'
            | b'H'
            | b'h'
            | b'V'
            | b'v'
            | b'C'
            | b'c'
            | b'S'
            | b's'
            | b'Q'
            | b'q'
            | b'T'
            | b't'
            | b'A'
            | b'a'
            | b'Z'
            | b'z'
    )
}

pub(crate) fn parse_path(d: &str) -> Result<Vec<RawSubpath>, MorphError> {
    let bytes = d.as_bytes();
    let n = bytes.len();
    let mut i = 0usize;
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut cur: Option<usize> = None;
    let mut cmd = 0u8;
    let mut px = 0.0;
    let mut py = 0.0;
    let mut prev_c = false;
    let mut prev_q = false;
    let mut started = false;
    let mut subs: Vec<RawSubpath> = Vec::new();

    let err = |i: usize, msg: &str| morph_err(format!("{msg} at d[{i}]"));

    let skip = |i: &mut usize| {
        while *i < n {
            match bytes[*i] {
                b' ' | b'\t' | b'\n' | b'\r' | 0x0c | b',' => *i += 1,
                _ => break,
            }
        }
    };

    let num = |i: &mut usize| -> Result<f64, MorphError> {
        skip(i);
        let start = *i;
        if *i < n && (bytes[*i] == b'+' || bytes[*i] == b'-') {
            *i += 1;
        }
        let mut dig = false;
        while *i < n && bytes[*i].is_ascii_digit() {
            *i += 1;
            dig = true;
        }
        if *i < n && bytes[*i] == b'.' {
            *i += 1;
            while *i < n && bytes[*i].is_ascii_digit() {
                *i += 1;
                dig = true;
            }
        }
        if !dig {
            return Err(err(*i, "expected number"));
        }
        if *i < n && (bytes[*i] == b'e' || bytes[*i] == b'E') {
            let save = *i;
            *i += 1;
            if *i < n && (bytes[*i] == b'+' || bytes[*i] == b'-') {
                *i += 1;
            }
            let mut ed = false;
            while *i < n && bytes[*i].is_ascii_digit() {
                *i += 1;
                ed = true;
            }
            if !ed {
                *i = save;
            }
        }
        let s = std::str::from_utf8(&bytes[start..*i]).unwrap_or("");
        s.parse::<f64>().map_err(|_| err(start, "expected number"))
    };

    let flag = |i: &mut usize| -> Result<u8, MorphError> {
        skip(i);
        if *i < n && (bytes[*i] == b'0' || bytes[*i] == b'1') {
            let v = if bytes[*i] == b'1' { 1 } else { 0 };
            *i += 1;
            return Ok(v);
        }
        Err(err(*i, "expected arc flag (0|1)"))
    };

    loop {
        skip(&mut i);
        if i >= n {
            break;
        }
        let ch = bytes[i];
        if is_command(ch) {
            cmd = ch;
            i += 1;
        } else if cmd == 0 {
            return Err(err(i, "path must start with M/m"));
        } else if cmd == b'M' {
            cmd = b'L';
        } else if cmd == b'm' {
            cmd = b'l';
        } else if cmd == b'Z' || cmd == b'z' {
            return Err(err(i, "stray data after Z"));
        }
        let rel = cmd >= b'a';
        let kind = if rel { cmd.to_ascii_uppercase() } else { cmd };
        let nx = |i: &mut usize, cx: f64, rel: bool| -> Result<f64, MorphError> {
            Ok(num(i)? + if rel { cx } else { 0.0 })
        };
        let ny = |i: &mut usize, cy: f64, rel: bool| -> Result<f64, MorphError> {
            Ok(num(i)? + if rel { cy } else { 0.0 })
        };

        match kind {
            b'M' => {
                started = true;
                let x = nx(&mut i, cx, rel)?;
                let y = ny(&mut i, cy, rel)?;
                cx = x;
                cy = y;
                sx = x;
                sy = y;
                subs.push(RawSubpath {
                    x0: x,
                    y0: y,
                    segs: Vec::new(),
                    closed: false,
                });
                cur = Some(subs.len() - 1);
                prev_c = false;
                prev_q = false;
            }
            b'L' => {
                if !started {
                    return Err(err(i, "path must start with M/m"));
                }
                if cur.is_none() {
                    subs.push(RawSubpath {
                        x0: cx,
                        y0: cy,
                        segs: Vec::new(),
                        closed: false,
                    });
                    cur = Some(subs.len() - 1);
                }
                let x = nx(&mut i, cx, rel)?;
                let y = ny(&mut i, cy, rel)?;
                if let Some(idx) = cur {
                    subs[idx].segs.push(RawSeg::L(x, y));
                }
                cx = x;
                cy = y;
                prev_c = false;
                prev_q = false;
            }
            b'H' => {
                if !started {
                    return Err(err(i, "path must start with M/m"));
                }
                if cur.is_none() {
                    subs.push(RawSubpath {
                        x0: cx,
                        y0: cy,
                        segs: Vec::new(),
                        closed: false,
                    });
                    cur = Some(subs.len() - 1);
                }
                let x = nx(&mut i, cx, rel)?;
                if let Some(idx) = cur {
                    subs[idx].segs.push(RawSeg::L(x, cy));
                }
                cx = x;
                prev_c = false;
                prev_q = false;
            }
            b'V' => {
                if !started {
                    return Err(err(i, "path must start with M/m"));
                }
                if cur.is_none() {
                    subs.push(RawSubpath {
                        x0: cx,
                        y0: cy,
                        segs: Vec::new(),
                        closed: false,
                    });
                    cur = Some(subs.len() - 1);
                }
                let y = ny(&mut i, cy, rel)?;
                if let Some(idx) = cur {
                    subs[idx].segs.push(RawSeg::L(cx, y));
                }
                cy = y;
                prev_c = false;
                prev_q = false;
            }
            b'C' | b'S' => {
                if !started {
                    return Err(err(i, "path must start with M/m"));
                }
                if cur.is_none() {
                    subs.push(RawSubpath {
                        x0: cx,
                        y0: cy,
                        segs: Vec::new(),
                        closed: false,
                    });
                    cur = Some(subs.len() - 1);
                }
                let (x1, y1) = if cmd == b'C' || cmd == b'c' {
                    (nx(&mut i, cx, rel)?, ny(&mut i, cy, rel)?)
                } else {
                    let x1 = if prev_c { 2.0 * cx - px } else { cx };
                    let y1 = if prev_c { 2.0 * cy - py } else { cy };
                    (x1, y1)
                };
                let x2 = nx(&mut i, cx, rel)?;
                let y2 = ny(&mut i, cy, rel)?;
                let x = nx(&mut i, cx, rel)?;
                let y = ny(&mut i, cy, rel)?;
                if let Some(idx) = cur {
                    subs[idx].segs.push(RawSeg::C(x1, y1, x2, y2, x, y));
                }
                px = x2;
                py = y2;
                cx = x;
                cy = y;
                prev_c = true;
                prev_q = false;
            }
            b'Q' | b'T' => {
                if !started {
                    return Err(err(i, "path must start with M/m"));
                }
                if cur.is_none() {
                    subs.push(RawSubpath {
                        x0: cx,
                        y0: cy,
                        segs: Vec::new(),
                        closed: false,
                    });
                    cur = Some(subs.len() - 1);
                }
                let (x1, y1) = if cmd == b'Q' || cmd == b'q' {
                    (nx(&mut i, cx, rel)?, ny(&mut i, cy, rel)?)
                } else {
                    let x1 = if prev_q { 2.0 * cx - px } else { cx };
                    let y1 = if prev_q { 2.0 * cy - py } else { cy };
                    (x1, y1)
                };
                let x = nx(&mut i, cx, rel)?;
                let y = ny(&mut i, cy, rel)?;
                if let Some(idx) = cur {
                    subs[idx].segs.push(RawSeg::Q(x1, y1, x, y));
                }
                px = x1;
                py = y1;
                cx = x;
                cy = y;
                prev_c = false;
                prev_q = true;
            }
            b'A' => {
                if !started {
                    return Err(err(i, "path must start with M/m"));
                }
                if cur.is_none() {
                    subs.push(RawSubpath {
                        x0: cx,
                        y0: cy,
                        segs: Vec::new(),
                        closed: false,
                    });
                    cur = Some(subs.len() - 1);
                }
                let rx = num(&mut i)?;
                let ry = num(&mut i)?;
                let rot = num(&mut i)?;
                let large = flag(&mut i)?;
                let sweep = flag(&mut i)?;
                let x = nx(&mut i, cx, rel)?;
                let y = ny(&mut i, cy, rel)?;
                if let Some(idx) = cur {
                    subs[idx]
                        .segs
                        .push(RawSeg::A(rx, ry, rot, large, sweep, x, y));
                }
                cx = x;
                cy = y;
                prev_c = false;
                prev_q = false;
            }
            b'Z' => {
                if let Some(idx) = cur {
                    subs[idx].closed = true;
                }
                cur = None;
                cx = sx;
                cy = sy;
                prev_c = false;
                prev_q = false;
            }
            _ => return Err(err(i, &format!("unsupported command \"{}\"", cmd as char))),
        }
    }

    Ok(subs.into_iter().filter(|s| !s.segs.is_empty()).collect())
}
