//! 基于 CycloneDDS 内置主题的 DDS 网络发现模块。
//!
//! 使用 DCPS 内置主题读取器扫描网络中存活的参与者、发布者和订阅者实体。

use std::ffi::CStr;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use regex::Regex;
use zenrc_dds::{
    dds_builtintopic_endpoint_t, dds_builtintopic_participant_t,
    dds_create_participant, dds_create_reader, dds_delete, dds_domainid_t, dds_entity_t,
    dds_get_guid, dds_guid_t, dds_return_loan, dds_sample_info_t, dds_take,
};

// CycloneDDS 内置主题伪句柄（来自 dds_basic_types.h）
const DDS_MIN_PSEUDO_HANDLE: dds_entity_t = 0x7fff0000_i32;
const DDS_BUILTIN_TOPIC_DCPSPARTICIPANT: dds_entity_t = DDS_MIN_PSEUDO_HANDLE + 1;
const DDS_BUILTIN_TOPIC_DCPSPUBLICATION: dds_entity_t = DDS_MIN_PSEUDO_HANDLE + 3;
const DDS_BUILTIN_TOPIC_DCPSSUBSCRIPTION: dds_entity_t = DDS_MIN_PSEUDO_HANDLE + 4;

const DDS_DOMAIN_DEFAULT: dds_domainid_t = u32::MAX;
const MAX_SAMPLES: usize = 256;

/// 将 GUID 字节数组格式化为 "XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX" 形式。
pub fn fmt_guid(v: &[u8; 16]) -> String {
    format!(
        "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        v[0], v[1], v[2], v[3],
        v[4], v[5],
        v[6], v[7],
        v[8], v[9],
        v[10], v[11], v[12], v[13], v[14], v[15],
    )
}

/// 参与者信息
#[derive(Debug, Clone)]
pub struct ParticipantInfo {
    pub guid: [u8; 16],
}

/// 端点（发布者或订阅者）信息
#[derive(Debug, Clone)]
pub struct EndpointInfo {
    pub guid: [u8; 16],
    pub participant_guid: [u8; 16],
    pub topic_name: String,
    pub type_name: String,
}

/// 发现结果
#[derive(Debug, Default)]
pub struct DiscoveryResult {
    pub participants: Vec<ParticipantInfo>,
    pub publications: Vec<EndpointInfo>,
    pub subscriptions: Vec<EndpointInfo>,
}

/// 创建 DDS 参与者（基础工厂）
fn create_dp(domain_id: Option<u32>) -> Result<dds_entity_t> {
    let id = domain_id.unwrap_or(DDS_DOMAIN_DEFAULT);
    let dp = unsafe {
        dds_create_participant(id, std::ptr::null(), std::ptr::null())
    };
    if dp < 0 {
        return Err(anyhow!("dds_create_participant failed: {}", dp));
    }
    Ok(dp)
}

/// 读取内置主题 reader 中的所有 participant 样本
unsafe fn drain_participants(
    reader: dds_entity_t,
    out: &mut Vec<ParticipantInfo>,
) {
    let mut samples: Vec<*mut dds_builtintopic_participant_t> =
        vec![std::ptr::null_mut(); MAX_SAMPLES];
    let mut infos: Vec<dds_sample_info_t> = vec![unsafe { std::mem::zeroed() }; MAX_SAMPLES];

    loop {
        let n = unsafe {
            dds_take(
                reader,
                samples.as_mut_ptr() as *mut *mut std::ffi::c_void,
                infos.as_mut_ptr(),
                MAX_SAMPLES,
                MAX_SAMPLES as u32,
            )
        };
        if n <= 0 {
            break;
        }
        for i in 0..n as usize {
            if !infos[i].valid_data {
                continue;
            }
            let p = unsafe { &*samples[i] };
            out.push(ParticipantInfo { guid: p.key.v });
        }
        // 归还样本贷款
        unsafe {
            dds_return_loan(reader, samples.as_mut_ptr() as *mut *mut std::ffi::c_void, n);
        }
    }
}

/// 读取内置主题 reader 中的所有 endpoint 样本
unsafe fn drain_endpoints(
    reader: dds_entity_t,
    out: &mut Vec<EndpointInfo>,
) {
    let mut samples: Vec<*mut dds_builtintopic_endpoint_t> =
        vec![std::ptr::null_mut(); MAX_SAMPLES];
    let mut infos: Vec<dds_sample_info_t> = vec![unsafe { std::mem::zeroed() }; MAX_SAMPLES];

    loop {
        let n = unsafe {
            dds_take(
                reader,
                samples.as_mut_ptr() as *mut *mut std::ffi::c_void,
                infos.as_mut_ptr(),
                MAX_SAMPLES,
                MAX_SAMPLES as u32,
            )
        };
        if n <= 0 {
            break;
        }
        for i in 0..n as usize {
            if !infos[i].valid_data {
                continue;
            }
            let ep = unsafe { &*samples[i] };
            let topic_name = if ep.topic_name.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(ep.topic_name).to_string_lossy().into_owned() }
            };
            let type_name = if ep.type_name.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(ep.type_name).to_string_lossy().into_owned() }
            };
            out.push(EndpointInfo {
                guid: ep.key.v,
                participant_guid: ep.participant_key.v,
                topic_name,
                type_name,
            });
        }
        unsafe {
            dds_return_loan(reader, samples.as_mut_ptr() as *mut *mut std::ffi::c_void, n);
        }
    }
}

// dds_return_loan is available via zenrc_dds::dds_return_loan

/// 执行一次有时限的 DDS 发现扫描。
///
/// `duration_secs` 为扫描等待时长（秒）。
/// `topic_filter` 为 topic 名称过滤字符串（空字符串表示不过滤）。
pub fn scan_network(
    domain_id: Option<u32>,
    duration: Duration,
    topic_filter: &str,
) -> Result<DiscoveryResult> {
    let dp = create_dp(domain_id)?;

    // 创建内置主题 reader
    let rp = unsafe {
        dds_create_reader(dp, DDS_BUILTIN_TOPIC_DCPSPARTICIPANT, std::ptr::null(), std::ptr::null())
    };
    let rw = unsafe {
        dds_create_reader(dp, DDS_BUILTIN_TOPIC_DCPSPUBLICATION, std::ptr::null(), std::ptr::null())
    };
    let rs = unsafe {
        dds_create_reader(dp, DDS_BUILTIN_TOPIC_DCPSSUBSCRIPTION, std::ptr::null(), std::ptr::null())
    };

    if rp < 0 || rw < 0 || rs < 0 {
        unsafe { dds_delete(dp) };
        return Err(anyhow!("Failed to create builtin topic readers"));
    }

    // 持续扫描直到超时
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }

    let mut result = DiscoveryResult::default();

    unsafe {
        drain_participants(rp, &mut result.participants);
        drain_endpoints(rw, &mut result.publications);
        drain_endpoints(rs, &mut result.subscriptions);
    }

    // 过滤掉扫描器自身的参与者及其内置端点
    let mut self_guid: dds_guid_t = unsafe { std::mem::zeroed() };
    unsafe { dds_get_guid(dp, &mut self_guid) };
    result.participants.retain(|p| p.guid != self_guid.v);
    result.publications.retain(|ep| ep.participant_guid != self_guid.v);
    result.subscriptions.retain(|ep| ep.participant_guid != self_guid.v);

    // 应用 topic 过滤（支持正则表达式；空字符串表示不过滤）
    if !topic_filter.is_empty() {
        let re = Regex::new(topic_filter).unwrap_or_else(|_| {
            // 若正则无效，降级为字面量子串匹配
            Regex::new(&regex::escape(topic_filter)).expect("escaped regex is always valid")
        });
        result.publications.retain(|ep| re.is_match(&ep.topic_name));
        result.subscriptions.retain(|ep| re.is_match(&ep.topic_name));
    }

    unsafe { dds_delete(dp) };

    Ok(result)
}
