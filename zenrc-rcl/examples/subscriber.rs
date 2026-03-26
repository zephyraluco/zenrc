use std::ffi::CString;
use std::ptr;

use zenrc_rcl::generated_types::std_msgs::msg::String as RosString;
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

        if rcl_node_init(
            &mut node,
            node_name.as_ptr(),
            namespace.as_ptr(),
            &mut context,
            &node_options,
        ) != 0
        {
            eprintln!("Failed to create node");
            rcl_shutdown(&mut context);
            return;
        }

        // 获取 std_msgs::String 的类型支持
        let type_support = RosString::get_ts();

        // 创建订阅者
        let mut subscription = rcl_get_zero_initialized_subscription();
        let topic_name = CString::new("chatter").unwrap();
        let subscription_options = rcl_subscription_get_default_options();

        if rcl_subscription_init(
            &mut subscription,
            &node,
            type_support,
            topic_name.as_ptr(),
            &subscription_options,
        ) != 0
        {
            eprintln!("Failed to create subscription");
            rcl_node_fini(&mut node);
            rcl_shutdown(&mut context);
            return;
        }

        println!("Subscriber started, listening to topic 'chatter'");

        // 创建等待集
        let mut wait_set = rcl_get_zero_initialized_wait_set();
        if rcl_wait_set_init(
            &mut wait_set,
            1,
            0,
            0,
            0,
            0,
            0,
            &mut context,
            rcutils_get_default_allocator(),
        ) != 0
        {
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
                    // 创建原生消息包装器来接收数据
                    let mut native_msg = NativeMsg::<RosString>::new();
                    let mut message_info: rmw_message_info_t = std::mem::zeroed();

                    if rcl_take(
                        &subscription,
                        native_msg.as_mut_ptr() as *mut _,
                        &mut message_info,
                        ptr::null_mut(),
                    ) == 0
                    {
                        // 从原生消息转换为 Rust 类型
                        let rust_msg = RosString::from_native(&native_msg);
                        println!("Received: {}", rust_msg.data);
                    }
                    // native_msg 会在作用域结束时自动释放
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
