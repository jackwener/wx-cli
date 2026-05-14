use anyhow::{bail, Result};

use super::{ImageKeyMaterial, ImageKeyProvider};

pub struct LinuxImageKeyProvider;

impl ImageKeyProvider for LinuxImageKeyProvider {
    fn get_key(&self, _wxid: &str) -> Result<ImageKeyMaterial> {
        bail!("Linux V2 图片 key 当前未实现；请先用 legacy/V1 图片或在 README 中标注 unsupported")
    }
}
