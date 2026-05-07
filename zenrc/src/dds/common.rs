use std::ffi::c_void;

use zenrc_dds::{
    RawMessageBridge, Sample, SampleInfo, dds_entity_t, dds_sample_info_t,
};

use super::error::{DdsError, Result};

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

pub(crate) fn collect_samples<T: RawMessageBridge>(
    n: i32,
    raw_samples: Vec<T::CStruct>,
    infos: Vec<dds_sample_info_t>,
    action: &'static str,
) -> Result<Vec<Sample<T>>> {
    if n < 0 {
        return Err(DdsError::RetCode(n, format!("{action} failed")));
    }
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

// ── take / read / peek 公共实现 ──────────────────────────────────────────────

pub(crate) fn take<T: RawMessageBridge>(
    reader: dds_entity_t,
    max: usize,
) -> Result<Vec<Sample<T>>> {
    take_with_mask(reader, max, zenrc_dds::DDS_ANY_STATE)
}

pub(crate) fn take_one<T: RawMessageBridge>(
    reader: dds_entity_t,
) -> Result<Option<Sample<T>>> {
    Ok(take(reader, 1)?.into_iter().next())
}

pub(crate) fn take_with_mask<T: RawMessageBridge>(
    reader: dds_entity_t,
    max: usize,
    mask: u32,
) -> Result<Vec<Sample<T>>> {
    read_or_take(reader, max, mask, true)
}

pub(crate) fn read<T: RawMessageBridge>(
    reader: dds_entity_t,
    max: usize,
) -> Result<Vec<Sample<T>>> {
    read_with_mask(reader, max, zenrc_dds::DDS_ANY_STATE)
}

pub(crate) fn read_one<T: RawMessageBridge>(
    reader: dds_entity_t,
) -> Result<Option<Sample<T>>> {
    Ok(read(reader, 1)?.into_iter().next())
}

pub(crate) fn read_with_mask<T: RawMessageBridge>(
    reader: dds_entity_t,
    max: usize,
    mask: u32,
) -> Result<Vec<Sample<T>>> {
    read_or_take(reader, max, mask, false)
}

pub(crate) fn peek<T: RawMessageBridge>(
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
    collect_samples(n, raw_samples, infos, "dds_peek")
}

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
    collect_samples(n, raw_samples, infos, "dds_take/read")
}