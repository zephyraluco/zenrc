use std::ffi::c_void;

use zenrc_dds::{
    CdrSample, LoanedSample, RawMessageBridge, Sample, SampleInfo, dds_entity_t, dds_sample_info_t, ddsi_serdata
};

use super::error::{DdsError, Result};

/// 为 take/read/peek 分配缓冲区并返回原始指针
pub(crate) fn allocate_sample_buffers<T: RawMessageBridge>(
    max: usize,
) -> (Vec<T::CStruct>, Vec<*mut c_void>, Vec<dds_sample_info_t>) {
    let mut raw_samples: Vec<T::CStruct> =
        (0..max).map(|_| unsafe { std::mem::zeroed() }).collect();
    let ptrs: Vec<*mut c_void> = raw_samples
        .iter_mut()
        .map(|sample| sample as *mut T::CStruct as *mut c_void)
        .collect();
    let infos = vec![unsafe { std::mem::zeroed() }; max];
    (raw_samples, ptrs, infos)
}

/// 将原始样本和信息转换为 Sample 结构体，并过滤掉无效数据
pub(crate) fn collect_samples<T: RawMessageBridge>(
    n: i32,
    raw_samples: Vec<T::CStruct>,
    infos: Vec<dds_sample_info_t>,
) -> Result<Vec<Sample<T>>> {
    let n = n as usize;

    let mut result = Vec::with_capacity(n);
    for (raw, raw_info) in raw_samples.into_iter().zip(infos.into_iter()).take(n) {
        if raw_info.valid_data {
            result.push(Sample {
                inner: T::from_raw(raw),
                info: SampleInfo::from(raw_info),
            });
        } else {
            let _ = T::from_raw(raw);
        }
    }
    Ok(result)
}

/// 取出最多 `max` 条样本（消耗，数据从缓存中移除）
pub fn take<T: RawMessageBridge>(
    reader: dds_entity_t,
    max: usize,
) -> Result<Vec<Sample<T>>> {
    take_with_mask(reader, max, zenrc_dds::DDS_ANY_STATE)
}

/// 取出单条样本，无数据时返回 `None`
pub fn take_one<T: RawMessageBridge>(
    reader: dds_entity_t,
) -> Result<Option<Sample<T>>> {
    Ok(take(reader, 1)?.into_iter().next())
}

/// 按状态掩码取出最多 `max` 条样本
pub fn take_with_mask<T: RawMessageBridge>(
    reader: dds_entity_t,
    max: usize,
    mask: u32,
) -> Result<Vec<Sample<T>>> {
    read_or_take(reader, max, mask, true)
}

/// 读取最多 `max` 条样本（数据保留在缓存中）
pub fn read<T: RawMessageBridge>(
    reader: dds_entity_t,
    max: usize,
) -> Result<Vec<Sample<T>>> {
    read_with_mask(reader, max, zenrc_dds::DDS_ANY_STATE)
}

/// 读取单条样本，无数据时返回 `None`（数据保留在缓存中）
pub fn read_one<T: RawMessageBridge>(
    reader: dds_entity_t,
) -> Result<Option<Sample<T>>> {
    Ok(read(reader, 1)?.into_iter().next())
}

/// 以借用方式读取最多 `max` 条样本（零拷贝，数据保留在缓存中）
pub fn read_wl<T: RawMessageBridge>(
    reader: dds_entity_t,
    max: usize,
) -> Result<Vec<LoanedSample<T>>> {
    if max == 0 {
        return Ok(Vec::new());
    }
    let mut ptrs = vec![std::ptr::null_mut::<c_void>(); max];
    let mut infos = vec![unsafe { std::mem::zeroed::<dds_sample_info_t>() }; max];
    let n = unsafe {
        zenrc_dds::dds_read_wl(reader, ptrs.as_mut_ptr(), infos.as_mut_ptr(), max as u32)
    };
    if n < 0 {
        return Err(DdsError::RetCode(n, "dds_read_wl failed".to_string()));
    }
    let result = ptrs.into_iter()
        .zip(infos.into_iter())
        .take(n as usize)
        .map(|(ptr, info)| LoanedSample::new(reader, ptr, SampleInfo::from(info)))
        .collect();
    Ok(result)
}

/// 以借用方式读取单条样本（零拷贝），无数据时返回 `None`
pub fn read_one_wl<T: RawMessageBridge>(
    reader: dds_entity_t,
) -> Result<Option<LoanedSample<T>>> {
    Ok(read_wl(reader, 1)?.into_iter().next())
}

/// 按状态掩码读取最多 `max` 条样本（数据保留在缓存中）
pub fn read_with_mask<T: RawMessageBridge>(
    reader: dds_entity_t,
    max: usize,
    mask: u32,
) -> Result<Vec<Sample<T>>> {
    read_or_take(reader, max, mask, false)
}

/// 预览最多 `max` 条样本（不修改读取状态，不改变已读/未读标记）
pub fn peek<T: RawMessageBridge>(
    reader: dds_entity_t,
    max: usize,
) -> Result<Vec<Sample<T>>> {
    if max == 0 {
        return Ok(Vec::new());
    }
    let (raw_samples, mut ptrs, mut infos) = allocate_sample_buffers::<T>(max);
    let n = unsafe {
        zenrc_dds::dds_peek(reader, ptrs.as_mut_ptr(), infos.as_mut_ptr(), max, max as u32)
    };
    collect_samples(n, raw_samples, infos)
}

/// 以借用方式取出最多 `max` 条样本（零拷贝，数据从缓存中移除）
pub fn take_wl<T: RawMessageBridge>(
    reader: dds_entity_t,
    max: usize,
) -> Result<Vec<LoanedSample<T>>> {
    if max == 0 {
        return Ok(Vec::new());
    }
    let mut ptrs = vec![std::ptr::null_mut::<c_void>(); max];
    let mut infos = vec![unsafe { std::mem::zeroed::<dds_sample_info_t>() }; max];
    let n = unsafe {
        zenrc_dds::dds_take_wl(reader, ptrs.as_mut_ptr(), infos.as_mut_ptr(), max as u32)
    };
    if n < 0 {
        return Err(DdsError::RetCode(n, "dds_take_wl failed".to_string()));
    }
    let result = ptrs.into_iter()
        .zip(infos.into_iter())
        .take(n as usize)
        .map(|(ptr, info)| LoanedSample::new(reader, ptr, SampleInfo::from(info)))
        .collect();
    Ok(result)
}

/// 以借用方式取出单条样本（零拷贝），无数据时返回 `None`
pub fn take_one_wl<T: RawMessageBridge>(
    reader: dds_entity_t,
) -> Result<Option<LoanedSample<T>>> {
    Ok(take_wl(reader, 1)?.into_iter().next())
}

/// 内部：按掩码读取或取出样本，`take=true` 时从缓存中移除
fn read_or_take<T: RawMessageBridge>(
    reader: dds_entity_t,
    max: usize,
    mask: u32,
    take: bool,
) -> Result<Vec<Sample<T>>> {
    if max == 0 {
        return Ok(Vec::new());
    }
    let (raw_samples, mut ptrs, mut infos) = allocate_sample_buffers::<T>(max);
    let n = unsafe {
        if take {
            zenrc_dds::dds_take_mask(reader, ptrs.as_mut_ptr(), infos.as_mut_ptr(), max, max as u32, mask)
        } else {
            zenrc_dds::dds_read_mask(reader, ptrs.as_mut_ptr(), infos.as_mut_ptr(), max, max as u32, mask)
        }
    };
    collect_samples(n, raw_samples, infos)
}

/// 为 CDR read/take/peek 分配缓冲区并返回原始指针。
fn allocate_cdr_buffers(max: usize) -> (Vec<*mut ddsi_serdata>, Vec<dds_sample_info_t>) {
    let ptrs = vec![std::ptr::null_mut(); max];
    let infos = vec![unsafe { std::mem::zeroed() }; max];
    (ptrs, infos)
}

/// 将原始 CDR 指针和信息转换为 `CdrSample`。
fn collect_cdr_samples(
    n: i32,
    raw_cdrs: Vec<*mut ddsi_serdata>,
    infos: Vec<dds_sample_info_t>,
) -> Result<Vec<CdrSample>> {
    if n < 0 {
        return Err(DdsError::RetCode(n, format!("collect_cdr_samples failed")));
    }
    let n = n as usize;

    let mut result = Vec::with_capacity(n);
    for (raw_cdr, raw_info) in raw_cdrs.into_iter().zip(infos.into_iter()).take(n) {
        if raw_info.valid_data && !raw_cdr.is_null() {
            result.push(CdrSample::new(raw_cdr, SampleInfo::from(raw_info)));
        }
    }
    Ok(result)
}

/// 预览最多 `max` 条 CDR 原始样本（不修改读取状态）
pub fn peek_cdr(reader: dds_entity_t, max: usize) -> Result<Vec<CdrSample>> {
    if max == 0 {
        return Ok(Vec::new());
    }
    let (mut raw_cdrs, mut infos) = allocate_cdr_buffers(max);
    let n = unsafe {
        zenrc_dds::dds_peekcdr(
            reader,
            raw_cdrs.as_mut_ptr(),
            max as u32,
            infos.as_mut_ptr(),
            zenrc_dds::DDS_ANY_STATE,
        )
    };
    collect_cdr_samples(n, raw_cdrs, infos)
}

/// 读取最多 `max` 条 CDR 原始样本（数据保留在缓存中）
pub fn read_cdr(reader: dds_entity_t, max: usize) -> Result<Vec<CdrSample>> {
    if max == 0 {
        return Ok(Vec::new());
    }
    let (mut raw_cdrs, mut infos) = allocate_cdr_buffers(max);
    let n = unsafe {
        zenrc_dds::dds_readcdr(
            reader,
            raw_cdrs.as_mut_ptr(),
            max as u32,
            infos.as_mut_ptr(),
            zenrc_dds::DDS_ANY_STATE,
        )
    };
    collect_cdr_samples(n, raw_cdrs, infos)
}

/// 取出最多 `max` 条 CDR 原始样本（数据从缓存中移除）
pub fn take_cdr(reader: dds_entity_t, max: usize) -> Result<Vec<CdrSample>> {
    if max == 0 {
        return Ok(Vec::new());
    }
    let (mut raw_cdrs, mut infos) = allocate_cdr_buffers(max);
    let n = unsafe {
        zenrc_dds::dds_takecdr(
            reader,
            raw_cdrs.as_mut_ptr(),
            max as u32,
            infos.as_mut_ptr(),
            zenrc_dds::DDS_ANY_STATE,
        )
    };
    collect_cdr_samples(n, raw_cdrs, infos)
}

/// 写入 CDR 序列化数据到 DDS writer
///
/// 接收 CdrSample（持有 ddsi_serdata），并通过 dds_writecdr 发送。
/// 成功后 CdrSample 的所有权转移给 DDS，drop 时不再释放。
pub fn write_cdr(writer: dds_entity_t, mut cdr: CdrSample) -> Result<()> {
    let ret = unsafe { zenrc_dds::dds_writecdr(writer, cdr.as_ptr()) };
    if ret != 0 {
        Err(DdsError::RetCode(ret, "dds_writecdr failed".to_string()))
    }else {
        Ok(())
    }
}

/// 转发 CDR 序列化数据到 DDS writer
///
/// 接收 CdrSample（持有 ddsi_serdata），并通过 dds_forwardcdr 转发。
/// 成功后 CdrSample 的所有权转移给 DDS，drop 时不再释放。
/// 通常用于网关/代理场景，直接转发网络接收的 CDR 数据。
pub fn forward_cdr(writer: dds_entity_t, mut cdr: CdrSample) -> Result<()> {
    let ret = unsafe { zenrc_dds::dds_forwardcdr(writer, cdr.as_ptr()) };
    if ret != 0 {
        Err(DdsError::RetCode(ret, "dds_forwardcdr failed".to_string()))
    } else {
        Ok(())
    }
}