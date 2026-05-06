#[path = "../src/dds/mod.rs"]
mod dds;

use std::time::Duration;

use dds::context::DdsContext;
use dds::qos::Qos;
use zenrc_dds::std_msgs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = DdsContext::new(0)?;

    let publisher = ctx.create_publisher::<std_msgs::msg::String>(
        "demo_chatter",
        Qos::sensor_data(),
    )?;
    let subscriber = ctx.create_subscription::<std_msgs::msg::String>(
        "demo_chatter",
        Qos::sensor_data(),
    )?;

    let sub_task = subscriber.set_event(|sample| {
        println!("[subscriber] 收到: {}", sample.data);
    })?;

    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut i = 0u32;
    loop {
        let msg = std_msgs::msg::String {
            data: format!("hello pub-sub #{i}"),
        };
        println!("[publisher] 发送: {}", msg.data);
        publisher.publish(msg)?;
        i += 1;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

}
