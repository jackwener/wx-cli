use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config;
use crate::crypto;
use crate::crypto::wal;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MtimeEntry {
    db_mt: u64,
    wal_mt: u64,
    path: String,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    db_mtime: u64,
    wal_mtime: u64,
    decrypted_path: PathBuf,
}

/// 解密后数据库的 mtime-aware 缓存
///
/// 当数据库文件（.db）或 WAL 文件（.db-wal）的 mtime 发生变化时，
/// 自动重新解密并更新缓存。跨进程重启可通过持久化 mtime 文件复用已解密的 DB。
pub struct DbCache {
    db_dir: PathBuf,
    cache_dir: PathBuf,
    mtime_file: PathBuf,
    all_keys: HashMap<String, String>, // rel_key -> enc_key(hex)
    inner: Arc<Mutex<HashMap<String, CacheEntry>>>,
}

impl DbCache {
    pub async fn new(
        db_dir: PathBuf,
        all_keys: HashMap<String, String>,
    ) -> Result<Self> {
        Self::with_dirs(db_dir, config::cache_dir(), config::mtime_file(), all_keys).await
    }

    /// 注入 `cache_dir` / `mtime_file`（测试用 + 生产 `new()` 复用）
    pub(crate) async fn with_dirs(
        db_dir: PathBuf,
        cache_dir: PathBuf,
        mtime_file: PathBuf,
        all_keys: HashMap<String, String>,
    ) -> Result<Self> {
        tokio::fs::create_dir_all(&cache_dir).await?;

        let cache = DbCache {
            db_dir,
            cache_dir,
            mtime_file,
            all_keys,
            inner: Arc::new(Mutex::new(HashMap::new())),
        };

        cache.load_persistent().await;
        Ok(cache)
    }

    /// 数据库根目录（即 `<wxchat_base>/db_storage`）。
    /// 上层（attachment resolver）需要 `db_dir.parent()` 来定位 `msg/attach/...` 解密图片。
    pub fn db_dir(&self) -> &Path {
        &self.db_dir
    }

    fn cache_file_path(&self, rel_key: &str) -> PathBuf {
        let hash = format!("{:x}", md5::compute(rel_key.as_bytes()));
        self.cache_dir.join(format!("{}.db", hash))
    }

    pub async fn needs_refresh(&self, rel_key: &str) -> bool {
        if !self.all_keys.contains_key(rel_key) {
            return false;
        }

        let db_path = self.db_dir.join(
            rel_key.replace('\\', std::path::MAIN_SEPARATOR_STR)
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        if !db_path.exists() {
            return false;
        }

        let wal_path = wal_path_for(&db_path);
        let db_mt = mtime_nanos(&db_path);
        let wal_mt = if wal_path.exists() { mtime_nanos(&wal_path) } else { 0 };

        let inner = self.inner.lock().await;
        match inner.get(rel_key) {
            Some(entry) => {
                entry.db_mtime != db_mt
                    || entry.wal_mtime != wal_mt
                    || !entry.decrypted_path.exists()
            }
            None => true,
        }
    }

    pub fn message_db_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .all_keys
            .keys()
            .filter(|k| {
                let normalized = k.replace('\\', "/");
                normalized.contains("message/message_")
                    && normalized.ends_with(".db")
                    && !normalized.contains("_fts")
                    && !normalized.contains("_resource")
            })
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    /// 从持久化文件加载 mtime 记录，复用未过期的解密文件
    async fn load_persistent(&self) {
        let mtime_file = &self.mtime_file;
        let content = match tokio::fs::read_to_string(&mtime_file).await {
            Ok(c) => c,
            Err(_) => return,
        };
        let saved: HashMap<String, MtimeEntry> = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return,
        };

        let mut inner = self.inner.lock().await;
        let mut reused = 0usize;
        for (rel_key, entry) in &saved {
            let dec_path = PathBuf::from(&entry.path);
            if !dec_path.exists() {
                continue;
            }
            let db_path = self.db_dir.join(rel_key.replace('\\', std::path::MAIN_SEPARATOR_STR).replace('/', std::path::MAIN_SEPARATOR_STR));
            let wal_path = wal_path_for(&db_path);

            let db_mt = mtime_nanos(&db_path);
            let _wal_mt = if wal_path.exists() { mtime_nanos(&wal_path) } else { 0 };

            // 只要主 .db 没变，就把 cached 产物载回来。
            // 如果 WAL mtime 变了，后续 `get()` 会自动走 Path 2：在已有 cached DB 上增量 apply_wal，
            // 而不是 daemon 重启后第一条请求又退回全量解密。
            if db_mt == entry.db_mt {
                inner.insert(rel_key.clone(), CacheEntry {
                    db_mtime: db_mt,
                    // 保留"cached 产物构建时看到的 wal_mtime"，让 `get()` 去比较当前 WAL
                    // 是否发生了变化，从而决定 exact-hit 还是 WAL 增量。
                    wal_mtime: entry.wal_mt,
                    decrypted_path: dec_path,
                });
                reused += 1;
            }
        }
        if reused > 0 {
            eprintln!("[cache] 复用 {} 个已解密 DB", reused);
        }
    }

    /// 持久化 mtime 记录
    async fn save_persistent(&self) {
        let mtime_file = &self.mtime_file;
        let inner = self.inner.lock().await;
        let data: HashMap<String, MtimeEntry> = inner.iter().map(|(k, v)| {
            (k.clone(), MtimeEntry {
                db_mt: v.db_mtime,
                wal_mt: v.wal_mtime,
                path: v.decrypted_path.to_string_lossy().into_owned(),
            })
        }).collect();
        drop(inner);

        if let Ok(json) = serde_json::to_string_pretty(&data) {
            let _ = tokio::fs::write(&mtime_file, json).await;
        }
    }

    /// 获取解密后的数据库路径
    ///
    /// 三种命中路径：
    /// 1. 主 `.db` 和 WAL mtime 都未变 → 直接返回缓存路径
    /// 2. 主 `.db` 未变、WAL mtime 变了 → 在已有 cached 产物上**增量** `apply_wal`
    ///    （apply_wal 是幂等的：旧帧 redo 同样的 page 写入，新帧追加生效；不重新 full_decrypt）
    /// 3. 主 `.db` mtime 变了 → 重新 `full_decrypt` + `apply_wal`
    ///
    /// WeChat 在写消息时只 append WAL（除非触发 checkpoint），因此 path 2 是常态；
    /// 这条路径把"每次请求都全量解密 ~1.8GB DB（~120s）"压到"只解 WAL 帧（典型 < 10s）"。
    pub async fn get(&self, rel_key: &str) -> Result<Option<PathBuf>> {
        let enc_key_hex = match self.all_keys.get(rel_key) {
            Some(k) => k.clone(),
            None => return Ok(None),
        };

        let db_path = self.db_dir.join(
            rel_key.replace('\\', std::path::MAIN_SEPARATOR_STR)
                   .replace('/', std::path::MAIN_SEPARATOR_STR)
        );
        if !db_path.exists() {
            return Ok(None);
        }

        let wal_path = wal_path_for(&db_path);
        let db_mt = mtime_nanos(&db_path);
        let wal_mt = if wal_path.exists() { mtime_nanos(&wal_path) } else { 0 };

        let cached = {
            let inner = self.inner.lock().await;
            inner.get(rel_key).cloned()
        };

        let enc_key_bytes = hex_to_32bytes(&enc_key_hex)
            .with_context(|| format!("密钥格式错误: {}", rel_key))?;

        // Path 1 / Path 2：主 .db mtime 未变且 cached 产物仍在
        if let Some(entry) = cached.as_ref() {
            if entry.db_mtime == db_mt && entry.decrypted_path.exists() {
                if entry.wal_mtime == wal_mt {
                    return Ok(Some(entry.decrypted_path.clone()));
                }

                // Path 2: WAL-only 变化 → 在 cached 产物上重新 apply_wal
                // 不存在的 WAL 也要更新 wal_mtime=0（虽然 SQLite 不会自发"主库不变 + WAL 清空"）
                let out_path = entry.decrypted_path.clone();
                let t0 = std::time::Instant::now();
                if wal_path.exists() {
                    let out_path2 = out_path.clone();
                    let wal_path2 = wal_path.clone();
                    let key_copy = enc_key_bytes;
                    tokio::task::spawn_blocking(move || {
                        wal::apply_wal(&wal_path2, &out_path2, &key_copy)
                    }).await??;
                }
                eprintln!("[cache] WAL 增量 {} ({}ms)", rel_key, t0.elapsed().as_millis());

                {
                    let mut inner = self.inner.lock().await;
                    inner.insert(rel_key.to_string(), CacheEntry {
                        db_mtime: db_mt,
                        wal_mtime: wal_mt,
                        decrypted_path: out_path.clone(),
                    });
                }
                self.save_persistent().await;
                return Ok(Some(out_path));
            }
        }

        // Path 3: 主 .db 变了 / 缓存 miss → 全量解密
        let out_path = self.cache_file_path(rel_key);
        let t0 = std::time::Instant::now();
        let db_path2 = db_path.clone();
        let out_path2 = out_path.clone();
        let key_copy = enc_key_bytes;
        tokio::task::spawn_blocking(move || {
            crypto::full_decrypt(&db_path2, &out_path2, &key_copy)
        }).await??;

        if wal_path.exists() {
            let out_path3 = out_path.clone();
            let wal_path3 = wal_path.clone();
            let key_copy2 = enc_key_bytes;
            tokio::task::spawn_blocking(move || {
                wal::apply_wal(&wal_path3, &out_path3, &key_copy2)
            }).await??;
        }

        eprintln!("[cache] 全量解密 {} ({}ms)", rel_key, t0.elapsed().as_millis());

        {
            let mut inner = self.inner.lock().await;
            inner.insert(rel_key.to_string(), CacheEntry {
                db_mtime: db_mt,
                wal_mtime: wal_mt,
                decrypted_path: out_path.clone(),
            });
        }

        self.save_persistent().await;
        Ok(Some(out_path))
    }
}

pub(super) fn mtime_nanos(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64)
        .unwrap_or(0)
}

/// `foo/bar.db` → `foo/bar.db-wal`（用 OsString 拼接，避免 display() 的 UTF-8 问题）
fn wal_path_for(db_path: &Path) -> PathBuf {
    let mut name = db_path.file_name().unwrap_or_default().to_os_string();
    name.push("-wal");
    db_path.with_file_name(name)
}

fn hex_to_32bytes(s: &str) -> Result<[u8; 32]> {
    if s.len() != 64 {
        anyhow::bail!("密钥 hex 长度应为 64，实际为 {}", s.len());
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)
            .with_context(|| format!("非法 hex 字符 at {}", i * 2))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 64 字符 hex（不需要是真 SQLCipher key — 仅用来证明"是否触发了 full_decrypt"）
    const FAKE_KEY_HEX: &str =
        "0000000000000000000000000000000000000000000000000000000000000000";

    /// 路径区分约定：
    /// - 完全 hit / WAL 增量 → `decrypted_path` **内容不变**
    /// - 全量解密 → `crypto::full_decrypt` 把 cached file **重写为 PAGE_SZ 倍数**
    ///   （fake key 解出 4096 字节垃圾，但仍写入 — 不验证内容合法性）
    /// 因此用 cached file 的"size 是否被改"来判断走了哪条路径。
    const ORIGINAL_CACHED_BYTES: &[u8] = b"original cached contents";

    fn unique_tmpdir(tag: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("wx-cli-cache-test-{}-{}-{}", tag, pid, nanos));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// 准备一份 "DbCache 已经 reuse 了 cached 解密产物" 的初始状态。
    /// 返回 (cache, db_path, decrypted_path, mtime_file, rel_key)。
    async fn setup_seeded_cache(tag: &str) -> (DbCache, PathBuf, PathBuf, PathBuf, String) {
        let root = unique_tmpdir(tag);
        let db_dir = root.join("db_storage");
        let cache_dir = root.join("cache");
        std::fs::create_dir_all(&db_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        let rel_key = "message_0.db".to_string();
        let db_path = db_dir.join(&rel_key);
        std::fs::write(&db_path, b"fake encrypted db").unwrap();

        let cached_hash = format!("{:x}", md5::compute(rel_key.as_bytes()));
        let decrypted_path = cache_dir.join(format!("{}.db", cached_hash));
        std::fs::write(&decrypted_path, ORIGINAL_CACHED_BYTES).unwrap();

        let db_mt = mtime_nanos(&db_path);
        let mtime_file = cache_dir.join("_mtimes.json");
        let payload = serde_json::to_string(&serde_json::json!({
            &rel_key: {
                "db_mt": db_mt,
                "wal_mt": 0u64,
                "path": decrypted_path.display().to_string(),
            }
        }))
        .unwrap();
        std::fs::write(&mtime_file, payload).unwrap();

        let mut all_keys = HashMap::new();
        all_keys.insert(rel_key.clone(), FAKE_KEY_HEX.to_string());
        let cache = DbCache::with_dirs(db_dir, cache_dir, mtime_file.clone(), all_keys)
            .await
            .unwrap();

        (cache, db_path, decrypted_path, mtime_file, rel_key)
    }

    #[tokio::test]
    async fn exact_mtime_hit_skips_decrypt() {
        let (cache, _db_path, decrypted_path, _mtime_file, rel_key) =
            setup_seeded_cache("exact").await;

        let p = cache.get(&rel_key).await.unwrap().expect("cache should hit");
        assert_eq!(p, decrypted_path);

        // 完全 hit → cached file 内容不应被改
        let body = std::fs::read(&decrypted_path).unwrap();
        assert_eq!(body, ORIGINAL_CACHED_BYTES);
    }

    #[tokio::test]
    async fn wal_only_change_uses_incremental_path() {
        // 自己构造（不走 setup_seeded_cache）以便初始 mtime.json 同时写 db_mt 和 wal_mt
        let root = unique_tmpdir("walonly");
        let db_dir = root.join("db_storage");
        let cache_dir = root.join("cache");
        std::fs::create_dir_all(&db_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        let rel_key = "message_0.db".to_string();
        let db_path = db_dir.join(&rel_key);
        std::fs::write(&db_path, b"fake encrypted db").unwrap();

        let wal_path = wal_path_for(&db_path);
        std::fs::write(&wal_path, [0u8; 31]).unwrap(); // ≤ WAL_HDR_SZ=32 → apply_wal noop

        let cached_hash = format!("{:x}", md5::compute(rel_key.as_bytes()));
        let decrypted_path = cache_dir.join(format!("{}.db", cached_hash));
        std::fs::write(&decrypted_path, ORIGINAL_CACHED_BYTES).unwrap();

        let db_mt = mtime_nanos(&db_path);
        let wal_mt0 = mtime_nanos(&wal_path);
        let mtime_file = cache_dir.join("_mtimes.json");
        let payload = serde_json::to_string(&serde_json::json!({
            &rel_key: {
                "db_mt": db_mt,
                "wal_mt": wal_mt0,
                "path": decrypted_path.display().to_string(),
            }
        }))
        .unwrap();
        std::fs::write(&mtime_file, payload).unwrap();

        let mut all_keys = HashMap::new();
        all_keys.insert(rel_key.clone(), FAKE_KEY_HEX.to_string());
        let cache = DbCache::with_dirs(db_dir, cache_dir, mtime_file, all_keys)
            .await
            .unwrap();

        // 第一次：完全 hit
        let p1 = cache.get(&rel_key).await.unwrap().expect("first get hits");
        assert_eq!(p1, decrypted_path);
        assert_eq!(std::fs::read(&decrypted_path).unwrap(), ORIGINAL_CACHED_BYTES);

        // bump WAL mtime（重写仍 31 bytes，apply_wal 仍 noop）
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&wal_path, [0xffu8; 31]).unwrap();
        let wal_mt1 = mtime_nanos(&wal_path);
        assert_ne!(wal_mt0, wal_mt1, "rewriting WAL should bump mtime");

        // 第二次：WAL 增量路径
        // 如果错误地走 full_decrypt → cached file 大小会被重写为 ≥ PAGE_SZ
        let p2 = cache
            .get(&rel_key)
            .await
            .unwrap()
            .expect("WAL-incremental path should produce path");
        assert_eq!(p2, decrypted_path);

        let body = std::fs::read(&decrypted_path).unwrap();
        assert_eq!(
            body, ORIGINAL_CACHED_BYTES,
            "WAL-incremental should NOT rewrite cached file"
        );
    }

    #[tokio::test]
    async fn db_mtime_change_triggers_full_decrypt() {
        let (cache, db_path, decrypted_path, _mtime_file, rel_key) =
            setup_seeded_cache("dbchange").await;

        // bump 主 .db 的 mtime（重写一份不同 bytes）
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&db_path, b"different fake encrypted bytes").unwrap();
        assert_ne!(
            mtime_nanos(&db_path),
            cache.inner.lock().await.get(&rel_key).unwrap().db_mtime,
            "rewriting db file should bump mtime"
        );

        // 走 full_decrypt 路径 → fake key 不会让 full_decrypt 失败（它不验证内容），
        // 但会把 cached file 重写为 PAGE_SZ 倍数。原始内容是 24 bytes，重写后应该 ≥ 4096 bytes。
        let p = cache
            .get(&rel_key)
            .await
            .unwrap()
            .expect("cache should produce path");
        assert_eq!(p, decrypted_path);

        let new_size = std::fs::metadata(&decrypted_path).unwrap().len() as usize;
        assert!(
            new_size >= crate::crypto::PAGE_SZ,
            "expected full_decrypt to rewrite cached file to PAGE_SZ multiple, got size={}",
            new_size,
        );
    }

    #[tokio::test]
    async fn restart_with_wal_change_still_reuses_cached_db_then_applies_wal() {
        let root = unique_tmpdir("restart-wal");
        let db_dir = root.join("db_storage");
        let cache_dir = root.join("cache");
        std::fs::create_dir_all(&db_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        let rel_key = "message_0.db".to_string();
        let db_path = db_dir.join(&rel_key);
        std::fs::write(&db_path, b"fake encrypted db").unwrap();

        let wal_path = wal_path_for(&db_path);
        std::fs::write(&wal_path, [0u8; 31]).unwrap(); // WAL 增量仍是 noop

        let cached_hash = format!("{:x}", md5::compute(rel_key.as_bytes()));
        let decrypted_path = cache_dir.join(format!("{}.db", cached_hash));
        std::fs::write(&decrypted_path, ORIGINAL_CACHED_BYTES).unwrap();

        let db_mt = mtime_nanos(&db_path);
        let wal_mt0 = mtime_nanos(&wal_path);
        let mtime_file = cache_dir.join("_mtimes.json");
        let payload = serde_json::to_string(&serde_json::json!({
            &rel_key: {
                "db_mt": db_mt,
                "wal_mt": wal_mt0,
                "path": decrypted_path.display().to_string(),
            }
        }))
        .unwrap();
        std::fs::write(&mtime_file, payload).unwrap();

        // 模拟 daemon 重启前又有新消息写入 WAL
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&wal_path, [0xffu8; 31]).unwrap();
        let wal_mt1 = mtime_nanos(&wal_path);
        assert_ne!(wal_mt0, wal_mt1);

        let mut all_keys = HashMap::new();
        all_keys.insert(rel_key.clone(), FAKE_KEY_HEX.to_string());
        let cache = DbCache::with_dirs(db_dir, cache_dir, mtime_file, all_keys)
            .await
            .unwrap();

        let p = cache.get(&rel_key).await.unwrap().expect("cache should reuse persisted DB");
        assert_eq!(p, decrypted_path);
        let body = std::fs::read(&decrypted_path).unwrap();
        assert_eq!(
            body, ORIGINAL_CACHED_BYTES,
            "restart + WAL-only change should still reuse cached DB and avoid full_decrypt"
        );
    }
}
