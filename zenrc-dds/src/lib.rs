#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
include!(concat!(env!("OUT_DIR"), "/msg_bindings.rs"));
include!(concat!(env!("OUT_DIR"), "/safe_types.rs"));

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
            let mut a = std_msgs_msg_String_desc;
            a.m_typename = "std_msgs::msg::dds_::String_\0".as_ptr() as *const ::std::os::raw::c_char;
            // 创建 topic
            let topic_name = CString::new("rt/test_string").unwrap();
            let topic = dds_create_topic(
                participant,
                &a,
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
                // 用 safe 类型构造消息，通过 Into<StringRaw> 转换为持有 raw C 结构体的 holder
                let safe_msg = std_msgs::msg::String {
                    data: format!("hello #{seq}"),
                };
                let text = safe_msg.data.clone();
                let holder: crate::std_msgs_msg_String = safe_msg.into();
                let rc = dds_write(writer, &holder as *const _ as *const _);
                if rc != DDS_RETCODE_OK as dds_return_t {
                    eprintln!("dds_write failed (seq={seq}): {rc}");
                } else {
                    println!("sent: {text}");
                }
                seq += 1;
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}