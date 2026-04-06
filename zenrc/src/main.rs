#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::CString;
use std::sync::OnceLock;
use std::time::Duration;

use zenrc_dds::*; // C 绑定类型（供 msg_bindings/generate_types 使用）
use zenrc_dds::{
    DdsMsg,
    domain::DomainParticipant,
    qos::Qos,
};

include!(concat!(env!("OUT_DIR"), "/msg_bindings.rs"));
include!(concat!(env!("OUT_DIR"), "/generate_types.rs"));

// ─── DdsMsg impl for std_msgs_msg_String ──────────────────────────────────────
//
// 在 zenrc 侧实现 DdsMsg，使 create_publisher/create_subscription 可直接使用
// 原始 C 绑定类型，并通过 safe ↔ raw 转换处理字符串。
//
// m_typename 改写为 ROS2 DDS 惯用的 "dds_::" 格式，以便与标准 ROS2 节点互通。

// dds_topic_descriptor_t 内含裸指针，需要封装才能放入 OnceLock（要求 Send + Sync）。
// SAFETY: 描述符数据均为 'static 只读，多线程并发只读是安全的。
struct DescHolder(zenrc_dds::dds_topic_descriptor_t);
unsafe impl Send for DescHolder {}
unsafe impl Sync for DescHolder {}

fn string_desc() -> *const zenrc_dds::dds_topic_descriptor_t {
    static HOLDER: OnceLock<DescHolder> = OnceLock::new();
    &HOLDER
        .get_or_init(|| {
            // 从 C 库复制描述符，仅覆盖 m_typename
            let mut desc = unsafe { std_msgs_msg_String_desc };
            desc.m_typename = b"std_msgs::msg::dds_::String_\0".as_ptr()
                as *const ::std::os::raw::c_char;
            DescHolder(desc)
        })
        .0 as *const _
}

// SAFETY:
// - descriptor 指向 'static DescHolder 内部，与 std_msgs_msg_String 内存布局完全匹配。
// - free_contents 使用默认实现（dds_sample_free + DDS_FREE_CONTENTS），
//   仅在订阅侧 Sample<T>::drop 时调用，此时 data 指针由 DDS 分配；
//   发布侧借用 CString 临时指针，不会触发 drop 中的 free_contents。
unsafe impl DdsMsg for std_msgs_msg_String {
    fn descriptor() -> *const zenrc_dds::dds_topic_descriptor_t {
        string_desc()
    }
}

// ─── main ─────────────────────────────────────────────────────────────────────

fn main() {
    let dp = DomainParticipant::new(0).expect("创建域参与者失败");

    let publisher = dp
        .create_publisher::<std_msgs_msg_String>("rt/test_string", Qos::sensor_data())
        .expect("创建发布者失败");

    println!("Publisher ready, sending on topic 'rt/test_string' every 100ms ...");

    let mut seq: u64 = 0;
    loop {
        // 用安全类型构造消息
        let safe_msg = std_msgs::msg::String {
            data: format!("hello #{seq}"),
        };
        let text = safe_msg.data.clone();

        // Into<std_msgs_msg_String> 内部通过 CString::into_raw() 分配堆内存
        let mut raw: std_msgs_msg_String = safe_msg.into();
        publisher.publish(&raw).expect("发布失败");
        // 释放 into() 分配的 C 字符串
        unsafe {
            if !raw.data.is_null() {
                drop(CString::from_raw(raw.data));
                raw.data = std::ptr::null_mut();
            }
        }

        println!("sent: {text}");
        seq += 1;
        std::thread::sleep(Duration::from_millis(100));
    }
}
