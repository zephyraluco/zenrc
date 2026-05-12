mod dds;

use std::time::Duration;

use dds::context::DdsContext;
use dds::log;
use dds::qos::Qos;
use zenrc_dds::std_msgs;

#[tokio::main]
async fn main() {
    log::init();

    let ctx = DdsContext::new(0).expect("创建 DDS 上下文失败");

    // 工厂方法内部使用 ROS2 命名约定：
    //   请求主题： rq/echo_serviceRequest
    //   应答主题： rr/echo_serviceReply
    // 程序运行时 ros2 service list 可见 /echo_service
    //
    // handler 在用户自己的线程中运行，通过 next() 驱动
    let server = ctx
        .create_service::<std_msgs::msg::String, std_msgs::msg::String>(
            "echo_service",
            Qos::services_default(),
        )
        .expect("创建服务端失败");

    let _server_task = server
        .set_event(|sample| {
            println!("[服务端] 收到请求: \"{}\"", sample.data);
            std_msgs::msg::String {
                data: sample.data.to_uppercase(),
            }
        })
        .expect("注册服务事件回调失败");

    let client = ctx
        .create_client::<std_msgs::msg::String, std_msgs::msg::String>(
            "echo_service",
            Qos::services_default(),
        )
        .expect("创建客户端失败");

    println!("Service/Client 已就绪，服务名: 'echo_service'");

    // 等待 DDS 发现
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut i = 0u32;
    loop {
        let req = std_msgs::msg::String {
            data: format!("hello #{i}"),
        };
        println!("[客户端] 发送请求: \"{}\"", req.data);
        match client.call(req, Duration::from_secs(5)) {
            Ok(Some(reply)) => println!("[客户端] 收到应答: \"{}\"", reply.data),
            Ok(None) => println!("[客户端] 请求 #{i} 超时"),
            Err(e) => eprintln!("[客户端] 调用错误: {e}"),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        i += 1;
    }
}

