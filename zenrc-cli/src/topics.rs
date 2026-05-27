//! Shared XTypes-based topic discovery for subscribe / publish / typeof.
//!
//! `discover_topic` scans DCPSPUBLICATION, finds the first endpoint that
//! matches `topic_name`, uses `dds_create_topic_descriptor` (XTypes) to build
//! a topic descriptor, creates the DDS topic, and returns
//! `(type_name, topic_entity, m_ops)`.

use std::ffi::{CStr, CString, c_void};
use std::time::Duration;

use zenrc_dds::{
    dds_builtintopic_endpoint_t, dds_builtintopic_get_endpoint_type_info,
    dds_create_reader, dds_create_topic, dds_create_topic_descriptor, dds_delete,
    dds_delete_topic_descriptor, dds_entity_t, dds_find_scope_t, dds_return_loan,
    dds_sample_info_t, dds_take,
};

const DDS_MIN_PSEUDO_HANDLE: dds_entity_t = 0x7fff0000_i32;
const DDS_BUILTIN_TOPIC_DCPSPUBLICATION: dds_entity_t = DDS_MIN_PSEUDO_HANDLE + 3;

/// Result returned by `discover_topic`.
pub struct DiscoveredTopic {
    /// Fully-qualified type name (e.g. `std_msgs::msg::String`).
    pub type_name: String,
    /// DDS topic entity handle — valid while the parent participant is alive.
    pub entity: dds_entity_t,
    /// Copy of `m_ops` from the topic descriptor (used for CDR encode/decode).
    pub m_ops: Vec<u32>,
}

/// Scan DCPSPUBLICATION for an endpoint on `topic_name`, build a topic
/// descriptor via XTypes TypeLookup, create the topic, and return a
/// `DiscoveredTopic`.
///
/// Returns `None` if no matching endpoint is found before `timeout` elapses,
/// or if the XTypes lookup fails.
pub fn discover_topic(
    dp: dds_entity_t,
    topic_name: &str,
    timeout: Duration,
) -> Option<DiscoveredTopic> {
    let rw = unsafe {
        dds_create_reader(
            dp,
            DDS_BUILTIN_TOPIC_DCPSPUBLICATION,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if rw < 0 {
        eprintln!("Failed to create DCPSPUBLICATION reader (code {rw})");
        return None;
    }

    let deadline = std::time::Instant::now() + timeout;
    let mut result: Option<DiscoveredTopic> = None;

    while result.is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));

        let mut samples: Vec<*mut dds_builtintopic_endpoint_t> =
            vec![std::ptr::null_mut(); 64];
        let mut infos: Vec<dds_sample_info_t> =
            vec![unsafe { std::mem::zeroed() }; 64];

        let n = unsafe {
            dds_take(
                rw,
                samples.as_mut_ptr() as *mut *mut c_void,
                infos.as_mut_ptr(),
                64,
                64,
            )
        };
        if n <= 0 {
            continue;
        }

        // Find the first valid endpoint matching topic_name
        let ep_idx = (0..n as usize).find(|&i| {
            infos[i].valid_data
                && !unsafe { &*samples[i] }.topic_name.is_null()
                && unsafe { CStr::from_ptr((*samples[i]).topic_name) }.to_string_lossy()
                    == topic_name
        });

        if let Some(i) = ep_idx {
            let ep = unsafe { &*samples[i] };
            let type_name = if ep.type_name.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(ep.type_name).to_string_lossy().into_owned() }
            };

            let mut type_info: *const zenrc_dds::dds_typeinfo_t = std::ptr::null();
            let rc = unsafe {
                dds_builtintopic_get_endpoint_type_info(samples[i], &mut type_info)
            };

            if rc == 0 && !type_info.is_null() {
                let remaining = deadline
                    .checked_duration_since(std::time::Instant::now())
                    .unwrap_or(Duration::from_secs(2));
                // Give XTypes TypeLookup at least 2 s
                let desc_timeout = (remaining.as_nanos() as i64).max(2_000_000_000);

                let mut desc: *mut zenrc_dds::dds_topic_descriptor_t = std::ptr::null_mut();

                // IMPORTANT: type_info points into the loan sample — call
                // dds_create_topic_descriptor while the loan is still held.
                let desc_rc = unsafe {
                    dds_create_topic_descriptor(
                        zenrc_dds::dds_find_scope_DDS_FIND_SCOPE_GLOBAL
                            as dds_find_scope_t,
                        dp,
                        type_info,
                        desc_timeout,
                        &mut desc,
                    )
                };

                // Copy m_ops out before the descriptor (and its loan) are freed
                let m_ops: Vec<u32> = if desc_rc == 0 && !desc.is_null() {
                    let d = unsafe { &*desc };
                    let n_ops = d.m_nops as usize;
                    if n_ops > 0 && !d.m_ops.is_null() {
                        unsafe { std::slice::from_raw_parts(d.m_ops, n_ops) }.to_vec()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                // Return the loan — type_info is no longer valid after this
                unsafe {
                    dds_return_loan(rw, samples.as_mut_ptr() as *mut *mut c_void, n);
                }

                if desc_rc == 0 && !desc.is_null() {
                    let topic_c = CString::new(topic_name).unwrap();
                    let te = unsafe {
                        dds_create_topic(
                            dp,
                            desc as *const _,
                            topic_c.as_ptr(),
                            std::ptr::null(),
                            std::ptr::null(),
                        )
                    };
                    unsafe { dds_delete_topic_descriptor(desc) };

                    if te >= 0 {
                        result = Some(DiscoveredTopic { type_name, entity: te, m_ops });
                    } else {
                        eprintln!("Failed to create topic from descriptor (code {te})");
                    }
                } else {
                    eprintln!(
                        "dds_create_topic_descriptor failed (code {desc_rc}). \
                         Is a publisher active on domain?"
                    );
                }
            } else {
                // No type_info — return loan and skip
                unsafe {
                    dds_return_loan(rw, samples.as_mut_ptr() as *mut *mut c_void, n);
                }
                eprintln!("Could not get type_info from endpoint (code {rc})");
            }

            // Whether we succeeded or failed, stop searching after first match
            break;
        } else {
            // No matching endpoint yet — return loan and retry
            unsafe {
                dds_return_loan(rw, samples.as_mut_ptr() as *mut *mut c_void, n);
            }
        }
    }

    unsafe { dds_delete(rw) };
    result
}
