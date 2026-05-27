//! `zenrc-cli subscribe <topic>` — 订阅指定 topic 并以 JSON 格式打印收到的数据。
//!
//! 使用 XTypes（dds_create_topic_descriptor）通过 DCPSPUBLICATION 发现的
//! type_info 动态构建 topic descriptor，再用 TypeObject + m_ops 解码 CDR → JSON。

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use zenrc_dds::{
    dds_create_participant, dds_create_reader, dds_create_waitset, dds_delete,
    dds_domainid_t, dds_entity_t, dds_get_typeinfo, dds_return_t, dds_sample_info_t,
    dds_set_status_mask, dds_waitset_attach, dds_waitset_wait, ddsi_serdata, DDS_ANY_STATE,
};

use crate::topics::discover_topic;
use crate::type_schema::{self, TypeSchema};

const DDS_DOMAIN_DEFAULT: dds_domainid_t = u32::MAX;
const MAX_CDR_SAMPLES: usize = 32;
const DDS_DATA_AVAILABLE_STATUS: u32 = 1 << 8;

// ddsi_serdata_size / ddsi_serdata_to_ser / ddsi_serdata_unref are
// DDS_INLINE_EXPORT symbols exported by libddsc.so but not in zenrc_dds.
unsafe extern "C" {
    fn ddsi_serdata_size(d: *const ddsi_serdata) -> u32;
    fn ddsi_serdata_to_ser(d: *const ddsi_serdata, off: usize, sz: usize, buf: *mut c_void);
    fn ddsi_serdata_unref(serdata: *mut ddsi_serdata);
    fn dds_takecdr(
        rd_or_cnd: dds_entity_t,
        buf: *mut *mut ddsi_serdata,
        maxs: u32,
        si: *mut dds_sample_info_t,
        mask: u32,
    ) -> dds_return_t;
}

pub fn run(domain_id: Option<u32>, runtime_secs: f64, topic_name: &str) -> Result<()> {
    let id = domain_id.unwrap_or(DDS_DOMAIN_DEFAULT);
    let dp = unsafe { dds_create_participant(id, std::ptr::null(), std::ptr::null()) };
    if dp < 0 {
        return Err(anyhow!("Failed to create participant (code {})", dp));
    }

    eprintln!("Discovering topic '{}'...", topic_name);

    let scan_dur = Duration::from_secs_f64((runtime_secs * 0.5).max(2.0).min(10.0));
    let discovered = match discover_topic(dp, topic_name, scan_dur) {
        Some(d) => d,
        None => {
            unsafe { dds_delete(dp) };
            return Err(anyhow!(
                "Could not discover topic '{}'. Is a publisher active on this domain?",
                topic_name
            ));
        }
    };

    eprintln!("Discovered type: {}", discovered.type_name);

    // Build TypeSchema: field names from TypeObject, field types from m_ops
    let schema = build_schema(dp, discovered.entity, &discovered.m_ops);
    if schema.has_fields() {
        if !schema.idl.is_empty() {
            eprintln!("IDL:\n{}", schema.idl);
        }
    } else {
        eprintln!("(TypeObject unavailable; showing CDR hex)");
    }

    let reader =
        unsafe { dds_create_reader(dp, discovered.entity, std::ptr::null(), std::ptr::null()) };
    if reader < 0 {
        unsafe { dds_delete(dp) };
        return Err(anyhow!("Failed to create reader (code {})", reader));
    }

    unsafe { dds_set_status_mask(reader, DDS_DATA_AVAILABLE_STATUS) };

    let ws = unsafe { dds_create_waitset(dp) };
    if ws < 0 {
        unsafe { dds_delete(dp) };
        return Err(anyhow!("Failed to create waitset"));
    }
    unsafe { dds_waitset_attach(ws, reader, reader as zenrc_dds::dds_attach_t) };

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || r.store(false, Ordering::SeqCst))?;

    eprintln!("Subscribing to '{}' (Ctrl+C to quit)...", topic_name);
    let mut sample_count = 0u64;

    while running.load(Ordering::SeqCst) {
        let mut xs: Vec<zenrc_dds::dds_attach_t> = vec![0; 4];
        let n = unsafe { dds_waitset_wait(ws, xs.as_mut_ptr(), xs.len(), 500_000_000) };
        if n <= 0 {
            continue;
        }

        let mut serdata_ptrs: Vec<*mut ddsi_serdata> =
            vec![std::ptr::null_mut(); MAX_CDR_SAMPLES];
        let mut infos: Vec<dds_sample_info_t> =
            vec![unsafe { std::mem::zeroed() }; MAX_CDR_SAMPLES];

        let taken = unsafe {
            dds_takecdr(
                reader,
                serdata_ptrs.as_mut_ptr(),
                MAX_CDR_SAMPLES as u32,
                infos.as_mut_ptr(),
                DDS_ANY_STATE,
            )
        };

        if taken < 0 {
            continue;
        }

        for i in 0..taken as usize {
            let ptr = serdata_ptrs[i];
            if ptr.is_null() {
                continue;
            }
            if !infos[i].valid_data {
                unsafe { ddsi_serdata_unref(ptr) };
                continue;
            }

            sample_count += 1;
            let sz = unsafe { ddsi_serdata_size(ptr) } as usize;

            let mut bytes = vec![0u8; sz];
            if sz > 0 {
                unsafe {
                    ddsi_serdata_to_ser(ptr, 0, sz, bytes.as_mut_ptr() as *mut c_void);
                }
            }
            unsafe { ddsi_serdata_unref(ptr) };

            // Decode CDR → JSON using schema, fall back to hex dump
            if schema.has_fields() {
                let val = type_schema::decode(&bytes, &schema);
                println!(
                    "[{}]  ts={}  {}",
                    sample_count,
                    infos[i].source_timestamp,
                    serde_json::to_string(&val).unwrap_or_else(|_| "<encode error>".to_owned())
                );
            } else {
                println!(
                    "[{}]  ts={}  {} bytes",
                    sample_count, infos[i].source_timestamp, bytes.len()
                );
                print_hex_dump(&bytes);
            }
        }
    }

    println!("\nReceived {} sample(s) total.", sample_count);
    unsafe { dds_delete(dp) };
    Ok(())
}

/// Build a `TypeSchema` from the discovered topic entity:
/// - field names from TypeObject (via C helper + XTypes)
/// - field types from m_ops
fn build_schema(dp: dds_entity_t, topic: dds_entity_t, m_ops: &[u32]) -> TypeSchema {
    let mut ti: *mut zenrc_dds::dds_typeinfo_t = std::ptr::null_mut();
    let rc = unsafe { dds_get_typeinfo(topic, &mut ti) };
    if rc != 0 || ti.is_null() {
        return TypeSchema::default();
    }

    let json_opt = unsafe { type_schema::query_typeobj_json(dp, ti, 5_000_000_000) };
    unsafe { zenrc_dds::dds_free_typeinfo(ti) };

    match json_opt.and_then(|j| TypeSchema::from_json(&j)) {
        Some(schema) => schema.with_m_ops(m_ops),
        None => TypeSchema::default(),
    }
}

/// Print a hex dump with 16 bytes per line and ASCII side-bar.
fn print_hex_dump(bytes: &[u8]) {
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02X}")).collect();
        let hex_str = format!("{:<47}", hex.join(" "));
        let ascii: String = chunk
            .iter()
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        println!("  {:04X}  {}  {}", i * 16, hex_str, ascii);
    }
}
