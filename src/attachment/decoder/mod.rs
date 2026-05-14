//! `.dat` 文件解码：根据 6B header magic 分发到具体 decoder。
//!
//! 三档：
//! | header[0..6]            | decoder           | 备注                                    |
//! |-------------------------|-------------------|-----------------------------------------|
//! | `07 08 V2 08 07`        | `v2`              | AES-128-ECB + XOR 混合，需要 image AES key |
//! | `07 08 V1 08 07`        | `v1_aes`          | 固定 AES key `cfcd208495d565ef`         |
//! | (其他, 通常无 magic)    | `v1_xor`          | legacy single-byte XOR，magic 自动探测  |
//!
//! 决策点放在 `dispatch`，让上层（`resolver` / CLI extract 命令）只跟一个入口打交道。

use anyhow::{anyhow, Result};

pub mod v1_xor;
pub mod v2;

/// 完整 V2 magic：`\x07\x08V2\x08\x07`
pub const V2_MAGIC: [u8; 6] = [0x07, 0x08, b'V', b'2', 0x08, 0x07];
/// 完整 V1 magic：`\x07\x08V1\x08\x07`
pub const V1_MAGIC: [u8; 6] = [0x07, 0x08, b'V', b'1', 0x08, 0x07];

/// 解码后的产物 + 探测出的图片格式
#[derive(Debug)]
pub struct DecodedImage {
    pub data: Vec<u8>,
    /// 推断出的图片扩展名（不带点），由 magic 决定。例如 "jpg" / "png" / "gif" / "webp" /
    /// "tif" / "bmp" / "hevc"（wxgf 容器）/ "bin"（未识别）
    pub format: &'static str,
    /// 解码器名称（"legacy_xor" / "v1_aes" / "v2"），用于 CLI 调试输出
    pub decoder: &'static str,
}

/// 由 caller 提供的 V2 image AES key（codex 的 `image_key` 模块负责拿到）。
/// 缺省时遇到 V2 文件会返回 `Err`，caller 可以拿到具体错误信息再处理。
#[derive(Debug, Clone, Copy, Default)]
pub struct V2KeyMaterial<'a> {
    pub aes_key: Option<&'a [u8; 16]>,
    /// XOR key — WeChat 4.x 默认 0x88，可 override
    pub xor_key: u8,
}

impl<'a> V2KeyMaterial<'a> {
    pub fn with_aes(key: &'a [u8; 16]) -> Self {
        Self { aes_key: Some(key), xor_key: 0x88 }
    }
}

/// 根据 `dat_bytes` 头部 magic 自动分发到对应 decoder。
///
/// `v2_key` 仅在文件是 V2 magic 时被消费。
pub fn dispatch(dat_bytes: &[u8], v2_key: V2KeyMaterial<'_>) -> Result<DecodedImage> {
    if dat_bytes.len() >= 6 {
        let head: &[u8; 6] = dat_bytes[..6].try_into().unwrap();
        if head == &V2_MAGIC {
            return v2::decode(dat_bytes, v2_key);
        }
        if head == &V1_MAGIC {
            // V1 fixed-AES: 固定 key = md5("0")[:16] = "cfcd208495d565ef"
            let fixed_key: [u8; 16] = *b"cfcd208495d565ef";
            return v2::decode(
                dat_bytes,
                V2KeyMaterial { aes_key: Some(&fixed_key), xor_key: v2_key.xor_key },
            )
            .map(|mut d| {
                d.decoder = "v1_aes";
                d
            });
        }
    }
    if dat_bytes.is_empty() {
        return Err(anyhow!("空 .dat 文件"));
    }
    v1_xor::decode(dat_bytes)
}

/// 从解密后的字节流头部探测图片格式扩展名。
///
/// 与上游 `decode_image.py::detect_image_format` 一致；新增 wxgf (HEVC 裸流) 的探测，
/// 因为 V2 解码后产物可能直接是 wxgf 容器。
pub fn detect_image_format(bytes: &[u8]) -> &'static str {
    if bytes.len() >= 4 && &bytes[..4] == b"wxgf" {
        return "hevc";
    }
    if bytes.len() >= 3 && bytes[..3] == [0xFF, 0xD8, 0xFF] {
        return "jpg";
    }
    if bytes.len() >= 4 && bytes[..4] == [0x89, 0x50, 0x4E, 0x47] {
        return "png";
    }
    if bytes.len() >= 3 && &bytes[..3] == b"GIF" {
        return "gif";
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return "webp";
    }
    if bytes.len() >= 4 && bytes[..4] == [0x49, 0x49, 0x2A, 0x00] {
        return "tif";
    }
    if bytes.len() >= 2 && &bytes[..2] == b"BM" {
        return "bmp";
    }
    "bin"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_basic_formats() {
        assert_eq!(detect_image_format(&[0xFF, 0xD8, 0xFF, 0xE0]), "jpg");
        assert_eq!(detect_image_format(&[0x89, 0x50, 0x4E, 0x47]), "png");
        assert_eq!(detect_image_format(b"GIF89a"), "gif");
        assert_eq!(detect_image_format(b"BM\0\0\0\0\0\0\0\0\0\0\0\0"), "bmp");
        let mut webp = b"RIFF\0\0\0\0WEBP".to_vec();
        webp.extend_from_slice(&[0; 4]);
        assert_eq!(detect_image_format(&webp), "webp");
        assert_eq!(detect_image_format(&[0x49, 0x49, 0x2A, 0x00]), "tif");
        assert_eq!(detect_image_format(b"wxgfXXXX"), "hevc");
        assert_eq!(detect_image_format(&[0, 0, 0, 0]), "bin");
    }
}
