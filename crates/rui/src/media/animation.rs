//! 多帧图像（GIF 等）解码结果。

/// 动画中的一帧 RGBA8。
#[derive(Clone, Debug)]
pub struct DecodedFrame {
    pub rgba: Vec<u8>,
    /// GIF 原生单位：厘秒（1/100 秒）。`0` 表示按 100ms 处理。
    pub delay_cs: u16,
}

/// 等尺寸帧序列。
#[derive(Clone, Debug)]
pub struct DecodedAnimation {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<DecodedFrame>,
}

impl DecodedAnimation {
    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    pub fn frame_delay_ms(&self, index: usize) -> u32 {
        let cs = self.frames.get(index).map(|f| f.delay_cs).unwrap_or(0);
        if cs == 0 {
            100
        } else {
            cs as u32 * 10
        }
    }
}
