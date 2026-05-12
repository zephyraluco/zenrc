#![allow(unused)]

//! CycloneDDS 日志配置
//!
//! 封装 `dds_set_log_mask` 与 `dds_log`，提供在程序启动时配置日志掩码
//! 以及向 CycloneDDS 日志系统写入消息的能力。
//!
//! 当前安装的 libddsc 仅将这两个函数导出为公开链接符号，其余日志 API
//! （`dds_set_log_file`、`dds_set_log_sink` 等）为内部 inline 函数，不可直接链接。

use std::ffi::CString;

use zenrc_dds::{dds_log, dds_set_log_mask};

// ─── 日志类别掩码 ─────────────────────────────────────────────────────────────

/// CycloneDDS 日志类别掩码常量，可按位组合后传给 [`set_log_mask`]。
pub mod mask {
    pub use zenrc_dds::DDS_LC_ALL as ALL;
    pub use zenrc_dds::DDS_LC_CONFIG as CONFIG;
    pub use zenrc_dds::DDS_LC_DATA as DATA;
    pub use zenrc_dds::DDS_LC_DISCOVERY as DISCOVERY;
    pub use zenrc_dds::DDS_LC_ERROR as ERROR;
    pub use zenrc_dds::DDS_LC_FATAL as FATAL;
    pub use zenrc_dds::DDS_LC_INFO as INFO;
    pub use zenrc_dds::DDS_LC_TIMING as TIMING;
    pub use zenrc_dds::DDS_LC_TRACE as TRACE;
    pub use zenrc_dds::DDS_LC_TRAFFIC as TRAFFIC;
    pub use zenrc_dds::DDS_LC_WARNING as WARNING;

    /// 默认掩码：仅输出 FATAL、ERROR、WARNING
    pub const DEFAULT: u32 = FATAL | ERROR | WARNING;
}

// ─── 公开 API ─────────────────────────────────────────────────────────────────

/// 设置 CycloneDDS 全局日志输出掩码。
///
/// 只有类别位在 `cats` 中置位的日志才会被输出，例如：
/// ```ignore
/// use zenrc::dds::log::{mask, set_log_mask};
/// set_log_mask(mask::ERROR | mask::WARNING);
/// ```
pub fn set_log_mask(cats: u32) {
    unsafe { dds_set_log_mask(cats) };
}

/// 向 CycloneDDS 全局日志系统写入一条消息。
///
/// - `cat` — 日志类别（`mask::*` 中的单个位）
/// - `msg` — 消息内容（无需包含换行符）
///
/// 若 `msg` 包含 NUL 字节则静默忽略。
pub fn write(cat: u32, msg: &str) {
    let Ok(cmsg) = CString::new(msg) else { return };
    unsafe {
        dds_log(
            cat,
            c"<rust>".as_ptr(),
            0,
            c"<rust>".as_ptr(),
            c"%s\n".as_ptr(),
            cmsg.as_ptr(),
        );
    }
}

/// 使用默认选项初始化 CycloneDDS 日志。
///
/// 将掩码设为 [`mask::DEFAULT`]（FATAL | ERROR | WARNING），
/// 应在创建 [`DdsContext`](crate::dds::context::DdsContext) 之前调用。
pub fn init() {
    set_log_mask(mask::DEFAULT);
}
