//! `zenrc-cli publish <topic>` — 向指定 topic 发布数据。
//!
//! 从 stdin 逐行读取 JSON 对象（如 `{"data":"hello"}`），编码为 CDR 并写入。
//! 输入 "exit" 或 EOF 时退出。

use std::io::{self, BufRead};
use std::time::Duration;

use anyhow::{anyhow, Result};
use zenrc_dds::{
    dds_create_participant, dds_create_writer, dds_delete, dds_domainid_t,
    dds_entity_t, dds_get_typeinfo,
};

use crate::topics::discover_topic;
use crate::type_schema::{self, TypeSchema};

const DDS_DOMAIN_DEFAULT: dds_domainid_t = u32::MAX;

pub fn run(domain_id: Option<u32>, runtime_secs: f64, topic_name: &str) -> Result<()> {
    let id = domain_id.unwrap_or(DDS_DOMAIN_DEFAULT);
    let dp = unsafe { dds_create_participant(id, std::ptr::null(), std::ptr::null()) };
    if dp < 0 {
        return Err(anyhow!("Failed to create participant"));
    }

    eprintln!("Discovering topic '{}'...", topic_name);
    let scan_dur = Duration::from_secs_f64((runtime_secs * 0.5).max(2.0).min(10.0));
    let discovered = match discover_topic(dp, topic_name, scan_dur) {
        Some(d) => d,
        None => {
            unsafe { dds_delete(dp) };
            return Err(anyhow!(
                "Topic '{}' not found. Make sure a matching publisher/subscriber is active.",
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
        eprintln!("Enter JSON per line (e.g. {{\"data\":\"hello\"}}), 'exit' to quit:");
    } else {
        eprintln!(
            "TypeObject unavailable. Enter raw CDR as hex per line \
             (4-byte header + payload), 'exit' to quit:"
        );
    }

    let writer =
        unsafe { dds_create_writer(dp, discovered.entity, std::ptr::null(), std::ptr::null()) };
    if writer < 0 {
        unsafe { dds_delete(dp) };
        return Err(anyhow!("Failed to create writer (code {})", writer));
    }

    let stdin = io::stdin();
    let mut count = 0u64;

    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();

        if trimmed.eq_ignore_ascii_case("exit") || trimmed.eq_ignore_ascii_case("quit") {
            break;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Parse input: JSON if schema available, hex otherwise
        let cdr_bytes: Vec<u8> = if schema.has_fields() {
            match serde_json::from_str::<serde_json::Value>(trimmed) {
                Ok(json) => type_schema::encode(&json, &schema),
                Err(e) => {
                    // Maybe it was already hex CDR — try parsing as hex
                    match parse_hex(trimmed) {
                        Ok(b) if b.len() >= 4 => b,
                        _ => {
                            eprintln!("Invalid JSON input: {e}");
                            continue;
                        }
                    }
                }
            }
        } else {
            match parse_hex(trimmed) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Invalid hex input: {e}");
                    continue;
                }
            }
        };

        if cdr_bytes.len() < 4 {
            eprintln!("CDR data must be at least 4 bytes (header + payload).");
            continue;
        }

        let rc = unsafe { type_schema::write_cdr_bytes(writer, &cdr_bytes) };

        if rc == 0 {
            count += 1;
            eprintln!("Published sample #{count}");
        } else {
            eprintln!("Write failed (rc={rc})");
        }
    }

    println!("Published {count} sample(s).");
    unsafe { dds_delete(dp) };
    Ok(())
}

/// Build a `TypeSchema` from the discovered topic entity.
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

fn parse_hex(s: &str) -> Result<Vec<u8>> {
    let s = s.replace([' ', '-', ':'], "");
    if s.len() % 2 != 0 {
        return Err(anyhow!("odd number of hex digits"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| anyhow!(e)))
        .collect()
}
