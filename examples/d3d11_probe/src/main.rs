//! D3D11 bring-up oracle: identical triangle/scissor/readback contract, two APIs.
//! Native mode is a direct D3D11 diagnostic, not a wgpu-hal backend implementation.
#[cfg(all(windows, feature = "angle"))]
mod angle;
#[cfg(all(windows, feature = "angle"))]
#[path = "../../../scripts/gpui-compare/angle_identity.rs"]
mod angle_identity;
#[cfg(windows)]
mod native;

#[cfg(all(windows, feature = "native-wgpu"))]
mod wgpu_native;

const SIZE: usize = 16;

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "native".into());
    let (mut report, pixels) = match mode.as_str() {
        "native" => native::run(false)?,
        #[cfg(feature = "native-wgpu")]
        "native-wgpu" => wgpu_native::run()?,
        #[cfg(feature = "native-hal")]
        "native-hal" => native::run(true)?,
        #[cfg(feature = "angle")]
        "angle" => angle::run()?,
        _ => {
            return Err(
                "use native; native-hal with feature native-hal; angle with feature angle".into(),
            )
        }
    };
    // Upper-left 8x8 scissor is shaded. The rest must retain the black clear.
    let mut errors = 0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let expected = if x < 8 && y < 8 {
                [64_u8, 128, 191, 255]
            } else {
                [0, 0, 0, 255]
            };
            let actual = &pixels[(y * SIZE + x) * 4..(y * SIZE + x + 1) * 4];
            errors += usize::from(actual.iter().zip(expected).any(|(a, b)| a.abs_diff(b) > 1));
        }
    }
    report["pixel_errors"] = errors.into();
    report["contract"] =
        "16x16 RGBA8Unorm; fullscreen triangle; top-left 8x8 scissor; staging readback".into();
    report["pixels"] = serde_json::json!(pixels);
    println!("{}", serde_json::to_string_pretty(&report)?);
    if errors != 0 {
        return Err(format!("pixel oracle failed: {errors}/256").into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("d3d11_probe requires Windows");
    std::process::exit(1);
}
