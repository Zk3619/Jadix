//! Borrow RGBA instead of cloning a DynamicImage for transport encoding.
use crate::RgbaFrame;
use std::{
    borrow::Cow,
    fmt::Write as _,
    io::{self, Write as _},
};

pub(crate) fn upload(
    frame: &RgbaFrame,
    id: u32,
    cells: ratatui::layout::Size,
    compressed: bool,
) -> io::Result<String> {
    let bytes = if compressed {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        z.write_all(frame.pixels())?;
        Cow::Owned(z.finish()?)
    } else {
        Cow::Borrowed(frame.pixels())
    };
    let mut result =
        String::with_capacity(bytes.len().div_ceil(3) * 4 + bytes.len().div_ceil(3072) * 100);
    let count = bytes.len().div_ceil(3072);
    for (index, chunk) in bytes.chunks(3072).enumerate() {
        result.push_str("\x1b_Gq=2,");
        if index == 0 {
            write!(
                result,
                "i={id},a=T,U=1,c={},r={},f=32,{}t=d,s={},v={},",
                cells.width,
                cells.height,
                if compressed { "o=z," } else { "" },
                frame.size().0,
                frame.size().1
            )
            .unwrap();
        }
        write!(result, "m={};", u8::from(index + 1 < count)).unwrap();
        base64_simd::STANDARD.encode_append(chunk, &mut result);
        result.push_str("\x1b\\");
    }
    Ok(result)
}
