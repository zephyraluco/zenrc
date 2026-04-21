use thiserror::Error;

/// DDS 操作错误类型
#[derive(Debug, Error)]
pub enum DdsError {
    /// CycloneDDS 返回了负值错误码
    #[error("DDS 错误码 {0}: {1}")]
    RetCode(i32, String),

    /// 字符串包含内部 NUL 字节
    #[error("字符串包含 NUL 字节: {0}")]
    NullStr(#[from] std::ffi::NulError),

    /// 空指针异常
    #[error("空指针异常: {0}")]
    NullPtr(String),
}

pub type Result<T, E = DdsError> = std::result::Result<T, E>;

/// 将 DDS 实体句柄转换为 Result，负值为错误
#[inline]
pub(crate) fn check_entity(entity: zenrc_dds::dds_entity_t) -> Result<zenrc_dds::dds_entity_t> {
    if entity >= 0 {
        Ok(entity)
    } else {
        Err(dds_err(entity))
    }
}

/// 将 DDS 返回码转换为 Result，负值为错误
#[inline]
pub(crate) fn check_ret(ret: zenrc_dds::dds_return_t) -> Result<()> {
    if ret >= 0 {
        Ok(())
    } else {
        Err(dds_err(ret))
    }
}

fn dds_err(code: i32) -> DdsError {
    let msg = unsafe {
        let ptr = zenrc_dds::dds_strretcode(code);
        if ptr.is_null() {
            "unknown error".to_owned()
        } else {
            std::ffi::CStr::from_ptr(ptr)
                .to_str()
                .unwrap_or("unknown error")
                .to_owned()
        }
    };
    DdsError::RetCode(code, msg)
}