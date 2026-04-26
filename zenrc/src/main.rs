mod dds;

use std::time::Duration;

use dds::context::DdsContext;
use dds::qos::Qos;
use zenrc_dds::std_msgs;

fn main() {
    let ctx = DdsContext::new(0).expect("创建 DDS 上下文失败");

    // 工厂方法内部使用 ROS2 命名约定：
    //   请求主题： rq/echo_serviceRequest
    //   应答主题： rr/echo_serviceReply
    // 程序运行时 ros2 service list 可见 /echo_service
    let server = ctx
        .create_service_server::<std_msgs::msg::String, std_msgs::msg::String>(
            "echo_service",
            Qos::services_default(),
        )
        .expect("创建服务端失败");

    let client = ctx
        .create_service_client::<std_msgs::msg::String, std_msgs::msg::String>(
            "echo_service",
            Qos::services_default(),
        )
        .expect("创建客户端失败");

    println!("Service/Client 已就绪，服务名: 'echo_service'");

    let server_thread = std::thread::spawn(move || {
        let mut handled = 0u32;
        loop {
            match server.spin_once(|req: std_msgs::msg::String| {
                println!("[服务端] 收到请求: \"{}\"", req.data);
                std_msgs::msg::String {
                    data: req.data.to_uppercase(),
                }
            }) {
                Ok(true) => handled += 1,
                Ok(false) => std::thread::sleep(Duration::from_millis(1)),
                Err(e) => {
                    eprintln!("[服务端] 错误: {e}");
                    break;
                }
            }
        }
        println!("[服务端] 已处理 {handled} 个请求，退出");
    });

    // 等待 DDS 发现
    std::thread::sleep(Duration::from_millis(500));

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
        std::thread::sleep(Duration::from_millis(50));
        i += 1;
    }

    server_thread.join().expect("服务端线程异常退出");
    println!("通信完成");
}

