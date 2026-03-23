use std::ffi::{CStr, CString};
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
        let node_name = CString::new("string_subscriber").unwrap();
        let namespace = CString::new("").unwrap();
        let node_options = rcl_node_get_default_options();

        if rcl_node_init(&mut node, node_name.as_ptr(), namespace.as_ptr(), &mut context, &node_options) != 0 {
            eprintln!("Failed to create node");
            rcl_shutdown(&mut context);
            return;
        }

        // 获取 std_msgs::String 的类型支持
        let type_support = rosidl_typesupport_c__get_message_type_support_handle__std_msgs__msg__String();

        // 创建订阅者
        let mut subscription = rcl_get_zero_initialized_subscription();
        let topic_name = CString::new("chatter").unwrap();
        let subscription_options = rcl_subscription_get_default_options();

        if rcl_subscription_init(&mut subscription, &node, type_support, topic_name.as_ptr(), &subscription_options) != 0 {
            eprintln!("Failed to create subscription");
            rcl_node_fini(&mut node);
            rcl_shutdown(&mut context);
            return;
        }

        println!("Subscriber started, listening to topic 'chatter'");

        // 创建等待集
        let mut wait_set = rcl_get_zero_initialized_wait_set();
        if rcl_wait_set_init(&mut wait_set, 1, 0, 0, 0, 0, 0, &mut context, rcutils_get_default_allocator()) != 0 {
            eprintln!("Failed to create wait set");
            rcl_subscription_fini(&mut subscription, &mut node);
            rcl_node_fini(&mut node);
            rcl_shutdown(&mut context);
            return;
        }

        // 接收消息循环
        loop {
            // 清空等待集
            if rcl_wait_set_clear(&mut wait_set) != 0 {
                eprintln!("Failed to clear wait set");
                break;
            }

            // 添加订阅者到等待集
            if rcl_wait_set_add_subscription(&mut wait_set, &subscription, ptr::null_mut()) != 0 {
                eprintln!("Failed to add subscription to wait set");
                break;
            }

            // 等待消息（超时 100ms）
            let timeout = 100_000_000; // 100ms in nanoseconds
            let ret = rcl_wait(&mut wait_set, timeout);

            if ret == 0 {
                // 检查订阅者是否有消息
                if !wait_set.subscriptions.is_null() && *wait_set.subscriptions != ptr::null() {
                    // 创建消息来接收数据
                    let msg = std_msgs__msg__String__create();
                    if msg.is_null() {
                        eprintln!("Failed to create message");
                        continue;
                    }

                    let mut message_info: rmw_message_info_t = std::mem::zeroed();

                    if rcl_take(&subscription, msg as *mut _, &mut message_info, ptr::null_mut()) == 0 {
                        // 读取字符串内容
                        let content = CStr::from_ptr((*msg).data.data);
                        let content_str = content.to_str().unwrap_or("");
                        println!("Received: {}", content_str);
                    }

                    // 销毁消息
                    std_msgs__msg__String__destroy(msg);
                }
            } else if ret != RCL_RET_TIMEOUT as i32 {
                eprintln!("Wait failed with error code: {}", ret);
            }
        }

        // 清理资源
        rcl_wait_set_fini(&mut wait_set);
        rcl_subscription_fini(&mut subscription, &mut node);
        rcl_node_fini(&mut node);
        rcl_shutdown(&mut context);
    }
}