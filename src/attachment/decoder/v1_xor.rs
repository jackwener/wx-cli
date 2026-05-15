//! Legacy single-byte XOR decoder（无 magic 头的旧 .dat）
//!
//! 算法：用已知图片 magic 反推 XOR key —— `key = file[0] ^ magic[0]`。
//! 然后用同一个 key 校验 `file[i] ^ key == magic[i]`，全部命中才接受这个 key。
//!
//! 优先级（按 magic 长度降序，避免短 magic 假阳性）：
//!   PNG (4) > GIF (4) > TIF (4) > WEBP (4, RIFF) > JPG (3) > BMP (2, 需额外校验)
//!
//! BMP 只有 2 字节 magic，假阳性高；额外用 BMP file header 里的
//! `bf_size`（offset 2, u32 LE）和 `bf_offset`（offset 10, u32 LE）做合理性校验：
//!   - `|bf_size - file_size| < 1024`（允许微小 padding 差）
//!   - `14 <= bf_offset <= 1078`（最大调色板 256*4 + header 14 = 1038，留点余量）

use anyhow::{anyhow, Result};

use super::{detect_image_format, DecodedImage};

const PNG: &[u8] = &[0x89, 0x50, 0x4E, 0x47];
const GIF: &[u8] = &[0x47, 0x49, 0x46, 0x38];
const TIF: &[u8] = &[0x49, 0x49, 0x2A, 0x00];
const WEBP_RIFF: &[u8] = &[0x52, 0x49, 0x46, 0x46];
const JPG: &[u8] = &[0xFF, 0xD8, 0xFF];
const BMP: &[u8] = &[0x42, 0x4D];

/// 在 `header` 上尝试一个固定 magic：返回 `Some(key)` 当且仅当所有字节都对得上。
fn try_magic(header: &[u8], magic: &[u8]) -> Option<u8> {
    if header.len() < magic.len() {
        return None;
    }
    let key = header[0] ^ magic[0];
    for i in 1..magic.len() {
        if header[i] ^ key != magic[i] {
            return None;
        }
    }
    Some(key)
}

/// 探测 XOR key。失败返回 `None`（caller 决定是不是错）。
pub fn detect_key(file_bytes: &[u8]) -> Option<u8> {
    if file_bytes.len() < 4 {
        return None;
    }
    let header = &file_bytes[..file_bytes.len().min(16)];

    // 先试 3+ 字节 magic
    for magic in [PNG, GIF, TIF, WEBP_RIFF, JPG] {
        if let Some(k) = try_magic(header, magic) {
            return Some(k);
        }
    }

    // 最后试 BMP（只有 2B magic，需额外校验）
    if let Some(k) = try_magic(header, BMP) {
        if header.len() >= 14 {
            // 解 BMP file header 14 字节
            let mut dec = [0u8; 14];
            for i in 0..14 {
                dec[i] = header[i] ^ k;
            }
            let bmp_size = u32::from_le_bytes([dec[2], dec[3], dec[4], dec[5]]);
            let bmp_offset = u32::from_le_bytes([dec[10], dec[11], dec[12], dec[13]]);
            let file_size = file_bytes.len() as u32;
            // 允许 1024 字节 padding 差；offset 在合理范围
            if file_size.abs_diff(bmp_size) < 1024 && (14..=1078).contains(&bmp_offset) {
                return Some(k);
            }
        }
    }

    None
}

/// XOR 解码整个 `.dat` 内容。
pub fn decode(file_bytes: &[u8]) -> Result<DecodedImage> {
    let key =
        detect_key(file_bytes).ok_or_else(|| anyhow!("legacy XOR: 无法识别图片 magic（key 探测失败）"))?;
    let data: Vec<u8> = file_bytes.iter().map(|b| b ^ key).collect();
    let format = detect_image_format(&data);
    if format == "bin" {
        return Err(anyhow!("legacy XOR: 解出 key=0x{:02x} 但产物 magic 不识别", key));
    }
    Ok(DecodedImage { data, format, decoder: "legacy_xor" })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 把一段 plaintext 用单字节 key XOR 加密，模拟 .dat 文件
    fn xor_encrypt(plain: &[u8], key: u8) -> Vec<u8> {
        plain.iter().map(|b| b ^ key).collect()
    }

    #[test]
    fn detect_jpg_key() {
        let plain = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
        let enc = xor_encrypt(&plain, 0x3C);
        assert_eq!(detect_key(&enc), Some(0x3C));
    }

    #[test]
    fn detect_png_key() {
        let mut plain = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        plain.extend_from_slice(&[0; 16]);
        let enc = xor_encrypt(&plain, 0xA5);
        assert_eq!(detect_key(&enc), Some(0xA5));
    }

    #[test]
    fn detect_gif_key() {
        let mut plain = b"GIF89a".to_vec();
        plain.extend_from_slice(&[0; 16]);
        let enc = xor_encrypt(&plain, 0x77);
        assert_eq!(detect_key(&enc), Some(0x77));
    }

    #[test]
    fn detect_webp_riff_key() {
        let mut plain = b"RIFF\x00\x00\x00\x00WEBP".to_vec();
        plain.extend_from_slice(&[0; 8]);
        let enc = xor_encrypt(&plain, 0x12);
        assert_eq!(detect_key(&enc), Some(0x12));
    }

    #[test]
    fn detect_tif_key() {
        let mut plain = vec![0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00];
        plain.extend_from_slice(&[0; 16]);
        let enc = xor_encrypt(&plain, 0xC3);
        assert_eq!(detect_key(&enc), Some(0xC3));
    }

    #[test]
    fn detect_bmp_with_valid_header() {
        // BMP 14B header: 'BM' + size(u32 LE) + reserved(2*u16) + offset(u32 LE)
        let mut plain = Vec::new();
        plain.extend_from_slice(b"BM");
        plain.extend_from_slice(&100u32.to_le_bytes()); // file_size = 100
        plain.extend_from_slice(&[0; 4]); // reserved
        plain.extend_from_slice(&54u32.to_le_bytes()); // pixel data offset = 54
        plain.resize(100, 0); // 整个文件 100 字节，匹配 file_size
        let enc = xor_encrypt(&plain, 0x55);
        assert_eq!(detect_key(&enc), Some(0x55));
    }

    #[test]
    fn reject_random_bytes() {
        // 全 0 文件：BMP 检测会算出 key = 0x42 ^ 0 = 0x42，
        // 但解密出的 BMP file_size = 0 vs file_size = 100，差距 > 1024 →
        // 应该 reject
        let bytes = vec![0u8; 100];
        assert_eq!(detect_key(&bytes), None);
    }

    #[test]
    fn decode_round_trip_jpg() {
        let mut plain = vec![0xFF, 0xD8, 0xFF, 0xE0];
        plain.extend_from_slice(b"JFIF padding here");
        let enc = xor_encrypt(&plain, 0xAB);
        let out = decode(&enc).unwrap();
        assert_eq!(out.format, "jpg");
        assert_eq!(out.decoder, "legacy_xor");
        assert_eq!(out.data, plain);
    }
}
