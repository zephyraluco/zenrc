use std::ffi::CString;
use std::ptr;
use zenrc_rcl::*;

fn main() {
    unsafe {
        // 初始化 RCL
        let mut context = rcl_get_zero_initialized_context();
        let mut init_options = rcl_get_zero_initialized_init_options();

        if rcl_init_options_init(&mut init_options, rcutils_get_default_allocator()) != 0 {
            eprintln!("Failed to initialize init options");
            return;
        }

        if rcl_init(0, ptr::null_mut(), &init_options, &mut context) != 0 {
            eprintln!("Failed to initialize RCL");
            return;
        }

        // 创建节点
        let mut node = rcl_get_zero_initialized_node();
        let node_name = CString::new("string_publisher").unwrap();
        let namespace = CString::new("").unwrap();
        let node_options = rcl_node_get_default_options();

        if rcl_node_init(&mut node, node_name.as_ptr(), namespace.as_ptr(), &mut context, &node_options) != 0 {
            eprintln!("Failed to create node");
            rcl_shutdown(&mut context);
            return;
        }

        // 获取 std_msgs::String 的类型支持
        let type_support = rosidl_typesupport_c__get_message_type_support_handle__std_msgs__msg__String();

        // 创建发布者
        let mut publisher = rcl_get_zero_initialized_publisher();
        let topic_name = CString::new("chatter").unwrap();
        let publisher_options = rcl_publisher_get_default_options();

        if rcl_publisher_init(&mut publisher, &node, type_support, topic_name.as_ptr(), &publisher_options) != 0 {
            eprintln!("Failed to create publisher");
            rcl_node_fini(&mut node);
            rcl_shutdown(&mut context);
            return;
        }

        println!("Publisher started, publishing to topic 'chatter'");

        // 发布消息循环
        let mut count = 0u32;
        loop {
            // 创建消息
            let msg = std_msgs__msg__String__create();
            if msg.is_null() {
                eprintln!("Failed to create message");
                std::thread::sleep(std::time::Duration::from_secs(1));
                continue;
            }

            // 设置消息内容
            let content = format!("Hello World: {}", count);
            let content_cstr = CString::new(content.clone()).unwrap();
            rosidl_runtime_c__String__assign(&mut (*msg).data, content_cstr.as_ptr());

            // 发布消息
            if rcl_publish(&publisher, msg as *const _, ptr::null_mut()) == 0 {
                println!("Published: {}", content);
            } else {
                eprintln!("Failed to publish message");
            }

            // 销毁消息
            std_msgs__msg__String__destroy(msg);

            count += 1;
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}