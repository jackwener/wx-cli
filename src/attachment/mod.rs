//! 聊天附件提取链路（图片 / 视频 / 语音 / 文件本体的本地解码）
//!
//! 整条链：
//!   message_N.db (Msg_<md5>) → message_resource.db (ChatName2Id + MessageResourceInfo)
//!     → packed_info protobuf md5 提取 → xwechat_files/<wxid>/msg/attach/.../Img/<md5>[_t|_h].dat
//!     → magic 分发 (legacy XOR / V1 fixed-AES / V2 AES+XOR) → 写出实际图片
//!
//! 模块切分：
//! - `attachment_id`：跨 IPC / CLI 的不透明 ID（base64url(json)）
//! - `resolver`：从 `attachment_id` 反查 message_resource.db，定位本地 .dat
//! - `decoder`：根据文件 magic 分发到具体解码器（V1 / V2 等）
//! - `image_key`：V2 image AES key 提取（macOS / Windows）
//!
//! V2 / image_key 模块由 codex 落地，先放空 stub 以便 V1 / resolver / CLI 不被 block。

// 此模块由分多个 PR/commit 增量启用：
// 1) 先落 attachment_id / decoder / resolver / image_key 骨架（本 commit）
// 2) IPC + CLI + daemon route 把它们串起来（后续 commit）
// 3) image_key 平台实现（codex 后续 commit）
// 在 step 1 完成、step 2 未到时，大量公开 API 仍未被引用，#[allow(dead_code)] 抑制噪音
#![allow(dead_code)]

pub mod attachment_id;
pub mod decoder;
pub mod resolver;
pub mod image_key;

pub use attachment_id::{AttachmentId, AttachmentKind};
