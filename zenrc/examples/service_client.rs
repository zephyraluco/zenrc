#[path = "../src/dds/mod.rs"]
mod dds;

use std::time::Duration;

use dds::context::DdsContext;
use dds::qos::Qos;
use zenrc_dds::std_msgs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = DdsContext::new(0)?;

    let server = ctx.create_service::<std_msgs::msg::String, std_msgs::msg::String>(
        "echo_service",
        Qos::services_default(),
    )?;
    let client = ctx.create_client::<std_msgs::msg::String, std_msgs::msg::String>(
        "echo_service",
        Qos::services_default(),
    )?;

    let server_task = server.set_event(|sample| {
        println!("[server] 收到请求: {}", sample.data);
        std_msgs::msg::String {
            data: sample.data.to_uppercase(),
        }
    })?;

    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut i = 0u32;

    loop {
        let req = std_msgs::msg::String {
            data: format!("hello service #{i}"),
        };
        println!("[client] 发送请求: {}", req.data);

        match client.call(req, Duration::from_secs(3))? {
            Some(reply) => println!("[client] 收到应答: {}", reply.data),
            None => println!("[client] 请求超时"),
        }
        i += 1;
        tokio::time::sleep(Duration::from_millis(120)).await;
    }

}
