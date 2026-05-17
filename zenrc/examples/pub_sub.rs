use std::time::Duration;

use zenrc::dds::context::DdsContext;
use zenrc::dds::qos::Qos;
use zenrc::msg::std_msgs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = DdsContext::new(0)?;

    let publisher = ctx.create_publisher::<std_msgs::msg::String>(
        "demo_chatter",
        Qos::sensor_data(),
    )?;
    let subscriber = ctx.create_subscriber::<std_msgs::msg::String>(
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
