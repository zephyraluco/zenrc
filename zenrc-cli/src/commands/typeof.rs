//! `zenrc-cli typeof <topic>` — 显示指定 topic 的 IDL 类型定义。
//!
//! 通过 XTypes TypeLookup 从 TypeObject 重建完整的 IDL struct 定义，
//! 与 cyclonedds-python `typeof` 命令输出对齐。

use std::time::Duration;

use anyhow::{anyhow, Result};
use zenrc_dds::{dds_create_participant, dds_delete, dds_domainid_t, dds_get_typeinfo};

use crate::topics::discover_topic;
use crate::type_schema;

const DDS_DOMAIN_DEFAULT: dds_domainid_t = u32::MAX;

pub fn run(domain_id: Option<u32>, runtime_secs: f64, topic_name: &str) -> Result<()> {
    let id = domain_id.unwrap_or(DDS_DOMAIN_DEFAULT);
    let dp = unsafe { dds_create_participant(id, std::ptr::null(), std::ptr::null()) };
    if dp < 0 {
        return Err(anyhow!("Failed to create participant (code {})", dp));
    }

    eprintln!("Discovering type for '{}' (XTypes)...", topic_name);

    let scan_dur = Duration::from_secs_f64(runtime_secs.max(2.0).min(30.0));
    let discovered = discover_topic(dp, topic_name, scan_dur);

    let info = match discovered {
        Some(d) => d,
        None => {
            unsafe { dds_delete(dp) };
            return Err(anyhow!(
                "Could not discover topic '{}'. Is any endpoint active?",
                topic_name
            ));
        }
    };

    eprintln!("Type: {}", info.type_name);

    // Get owned type_info from the topic entity (valid after loan is returned)
    let mut ti: *mut zenrc_dds::dds_typeinfo_t = std::ptr::null_mut();
    let rc = unsafe { dds_get_typeinfo(info.entity, &mut ti) };

    if rc == 0 && !ti.is_null() {
        // Give TypeLookup up to 5 s to retrieve the TypeObject
        let timeout_ns = 5_000_000_000i64;
        let json_opt = unsafe { type_schema::query_typeobj_json(dp, ti, timeout_ns) };

        unsafe { zenrc_dds::dds_free_typeinfo(ti) };

        match json_opt.and_then(|j| type_schema::TypeSchema::from_json(&j)) {
            Some(schema) if !schema.idl.is_empty() => {
                println!("Topic:  {}", topic_name);
                println!("Type:   {}", info.type_name);
                println!();
                println!("{}", schema.idl);
            }
            _ => {
                println!("Topic:  {}", topic_name);
                println!("Type:   {}", info.type_name);
                println!("(IDL reconstruction unavailable)");
            }
        }
    } else {
        println!("Topic:  {}", topic_name);
        println!("Type:   {}", info.type_name);
        println!("(could not retrieve TypeObject, rc={})", rc);
    }

    unsafe { dds_delete(dp) };
    Ok(())
}
