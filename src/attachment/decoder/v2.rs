//! V2 .dat 解码：`AES-128-ECB(PKCS7) + raw + XOR` 三段拼接。
//!
//! 文件结构（来自上游 `decode_image.py::v2_decrypt_file`）：
//!   `[6B magic V2/V1] [4B aes_size LE] [4B xor_size LE] [1B padding]`
//!   `[aligned_aes_size bytes AES-ECB ciphertext]`
//!   `[len - aligned_aes_size - xor_size bytes raw_data (不加密)]`
//!   `[xor_size bytes XOR (单字节 key)]`
//!
//! `aligned_aes_size`：把 `aes_size` 向上对齐到 16 的倍数；当 `aes_size` 本身是
//! 16 的倍数时，PKCS7 还会再加一整块 padding，所以再 +16。等价于
//! `aes_size + (16 - aes_size % 16)`。
//!
//! ⚠️ 此模块由 codex 落地完整 V2 实现 + image key 模块。当前只提供一个
//! `decode` 入口骨架，方便 v1_aes 路径（固定 key）和 dispatch 一起编译过。
//! `aes_key=None` 时返回带具体诊断信息的错误。

use anyhow::{anyhow, bail, Result};

use super::{detect_image_format, DecodedImage, V2KeyMaterial, V1_MAGIC, V2_MAGIC};

const HEADER_SIZE: usize = 15;

pub fn decode(file_bytes: &[u8], key: V2KeyMaterial<'_>) -> Result<DecodedImage> {
    if file_bytes.len() < HEADER_SIZE {
        bail!("V2 .dat: 文件过短（{} < {} 字节）", file_bytes.len(), HEADER_SIZE);
    }
    let magic: &[u8; 6] = file_bytes[..6].try_into().unwrap();
    if magic != &V2_MAGIC && magic != &V1_MAGIC {
        bail!("V2 .dat: header magic 不匹配 V1/V2");
    }

    let aes_key = key.aes_key.ok_or_else(|| {
        anyhow!("V2 .dat: 需要 image AES key（codex 的 image_key 模块尚未填充）")
    })?;

    let aes_size = u32::from_le_bytes(file_bytes[6..10].try_into().unwrap()) as usize;
    let xor_size = u32::from_le_bytes(file_bytes[10..14].try_into().unwrap()) as usize;

    // PKCS7 对齐：aes_size 不是 16 的倍数 → 向上对齐；是 16 的倍数 → 再加一整块
    let aligned_aes_size = aes_size + (16 - (aes_size % 16));

    let aes_end = HEADER_SIZE.checked_add(aligned_aes_size).ok_or_else(|| anyhow!("aes 段长度溢出"))?;
    if aes_end > file_bytes.len() {
        bail!(
            "V2 .dat: 头部宣称 aes_size={} (aligned={}) 超过文件长度 {}",
            aes_size,
            aligned_aes_size,
            file_bytes.len()
        );
    }
    let raw_end = file_bytes.len().checked_sub(xor_size).ok_or_else(|| {
        anyhow!("V2 .dat: 头部宣称 xor_size={} 超过文件长度 {}", xor_size, file_bytes.len())
    })?;
    if aes_end > raw_end {
        bail!(
            "V2 .dat: aes_end={} > raw_end={}（aes/xor 段重叠）",
            aes_end,
            raw_end
        );
    }

    // === AES-128-ECB 解密 + PKCS7 unpad ===
    let aes_data = &file_bytes[HEADER_SIZE..aes_end];
    let dec_aes = aes_ecb_decrypt_pkcs7(aes_key, aes_data)?;

    // === Raw 段（未加密） ===
    let raw_data = &file_bytes[aes_end..raw_end];

    // === XOR 段 ===
    let xor_data: Vec<u8> = file_bytes[raw_end..].iter().map(|b| b ^ key.xor_key).collect();

    let mut out = Vec::with_capacity(dec_aes.len() + raw_data.len() + xor_data.len());
    out.extend_from_slice(&dec_aes);
    out.extend_from_slice(raw_data);
    out.extend_from_slice(&xor_data);

    let format = detect_image_format(&out);
    if format == "bin" {
        bail!("V2 .dat: AES 解密成功但产物 magic 不识别（key 可能错）");
    }
    Ok(DecodedImage { data: out, format, decoder: "v2" })
}

/// AES-128-ECB 解密 + PKCS7 unpad。失败时返回 `Err`，不返回半结果。
///
/// 不引第三方 ECB 包；ECB 本身就是 block-by-block，手工跑就行。
/// PKCS7 padding 由本函数最后一段做 strict 校验：长度 1..=16，且尾部全是同值字节。
fn aes_ecb_decrypt_pkcs7(key: &[u8; 16], cipher: &[u8]) -> Result<Vec<u8>> {
    use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};
    if cipher.is_empty() || cipher.len() % 16 != 0 {
        bail!("AES 输入长度 {} 不是 16 的倍数", cipher.len());
    }
    let aes = aes::Aes128::new(key.into());
    let mut out = Vec::with_capacity(cipher.len());
    for chunk in cipher.chunks_exact(16) {
        let mut block = GenericArray::clone_from_slice(chunk);
        aes.decrypt_block(&mut block);
        out.extend_from_slice(&block);
    }
    let pad = *out.last().ok_or_else(|| anyhow!("AES PKCS7: 空输出"))? as usize;
    if pad == 0 || pad > 16 || pad > out.len() {
        bail!("AES PKCS7: 非法 padding 长度 {}", pad);
    }
    let tail = &out[out.len() - pad..];
    if !tail.iter().all(|&b| b as usize == pad) {
        bail!("AES PKCS7: padding 字节不一致");
    }
    out.truncate(out.len() - pad);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_file() {
        let r = decode(&[0u8; 4], V2KeyMaterial::default());
        assert!(r.is_err());
    }

    #[test]
    fn rejects_v2_without_key() {
        let mut buf = V2_MAGIC.to_vec();
        buf.extend_from_slice(&[0u8; HEADER_SIZE - 6]);
        let r = decode(&buf, V2KeyMaterial::default());
        let err = r.unwrap_err().to_string();
        assert!(err.contains("AES key"), "{}", err);
    }
}
