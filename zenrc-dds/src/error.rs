use thiserror::Error;

/// DDS 操作错误类型
#[derive(Debug, Error)]
pub enum DdsError {
    /// CycloneDDS 返回了负值错误码
    #[error("DDS 错误码 {0}: {1}")]
    RetCode(i32, String),

    /// 字符串包含内部 NUL 字节
    #[error("字符串包含 NUL 字节: {0}")]
    Nul(#[from] std::ffi::NulError),

    /// 无效 UTF-8
    #[error("UTF-8 错误: {0}")]
    Utf8(#[from] std::str::Utf8Error),
}

pub type Result<T, E = DdsError> = std::result::Result<T, E>;

/// 将 DDS 实体句柄转换为 Result，负值为错误
#[inline]
pub(crate) fn check_entity(entity: crate::dds_entity_t) -> Result<crate::dds_entity_t> {
    if entity >= 0 {
        Ok(entity)
    } else {
        Err(dds_err(entity))
    }
}

/// 将 DDS 返回码转换为 Result，负值为错误
#[inline]
pub(crate) fn check_ret(ret: crate::dds_return_t) -> Result<()> {
    if ret >= 0 {
        Ok(())
    } else {
        Err(dds_err(ret))
    }
}

fn dds_err(code: i32) -> DdsError {
    let msg = unsafe {
        let ptr = crate::dds_strretcode(code);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_entity_positive_is_ok() {
        assert_eq!(check_entity(42).unwrap(), 42);
    }

    #[test]
    fn check_entity_zero_is_ok() {
        assert_eq!(check_entity(0).unwrap(), 0);
    }

    #[test]
    fn check_entity_negative_returns_retcode_error() {
        let err = check_entity(-1).unwrap_err();
        match err {
            DdsError::RetCode(code, msg) => {
                assert_eq!(code, -1);
                assert!(!msg.is_empty());
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn check_ret_zero_is_ok() {
        check_ret(0).unwrap();
    }

    #[test]
    fn check_ret_positive_is_ok() {
        check_ret(5).unwrap();
    }

    #[test]
    fn check_ret_negative_returns_retcode_error() {
        let err = check_ret(-5).unwrap_err();
        match err {
            DdsError::RetCode(code, _) => assert_eq!(code, -5),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn dds_error_nul_from_conversion() {
        let nul_err = std::ffi::CString::new("ab\0cd").unwrap_err();
        let err = DdsError::Nul(nul_err);
        assert!(err.to_string().contains("NUL"));
    }

    #[test]
    fn dds_error_utf8_from_conversion() {
        let utf8_err = std::str::from_utf8(&[0xFF, 0xFE]).unwrap_err();
        let err = DdsError::Utf8(utf8_err);
        assert!(!err.to_string().is_empty());
    }
}
