#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
include!(concat!(env!("OUT_DIR"), "/msg_bindings.rs"));

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::ptr;
    use std::time::Duration;

    #[test]
    fn test_publish_forever() {
        unsafe {
            // 创建参与者（domain 0）
            let participant = dds_create_participant(0, ptr::null(), ptr::null());
            assert!(participant > 0, "Failed to create participant: {}", participant);

            // 创建 topic
            let topic_name = CString::new("rt/test_string").unwrap();
            let topic = dds_create_topic(
                participant,
                &std_msgs_msg_String_desc,
                topic_name.as_ptr(),
                ptr::null(),
                ptr::null(),
            );
            assert!(topic > 0, "Failed to create topic: {}", topic);

            // 创建 writer
            let writer = dds_create_writer(participant, topic, ptr::null(), ptr::null());
            assert!(writer > 0, "Failed to create writer: {}", writer);

            println!("Publisher ready, sending on topic 'rt/test_string' every 100ms ...");

            let mut seq: u64 = 0;
            loop {
                let payload = CString::new(format!("hello #{seq}")).unwrap();
                let mut msg = std_msgs_msg_String {
                    data: payload.as_ptr() as *mut _,
                };
                let rc = dds_write(writer, &mut msg as *mut _ as *const _);
                if rc != DDS_RETCODE_OK as dds_return_t {
                    eprintln!("dds_write failed (seq={seq}): {rc}");
                } else {
                    println!("sent: hello #{seq}");
                }
                seq += 1;
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}