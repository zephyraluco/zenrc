use zenrc::dds::context::{DOMAIN_DEFAULT, DdsContext};
use zenrc::dds::qos::Qos;
use zenrc::msg::std_msgs;

#[tokio::main]
async fn main() -> zenrc::dds::error::Result<()> {
    let ctx = DdsContext::new(DOMAIN_DEFAULT)?;
    let pub_ = ctx.create_publisher::<std_msgs::msg::String>("chatter", Qos::default())?;
    let sub = ctx.create_subscriber::<std_msgs::msg::String>("chatter", Qos::default())?;
    sub.set_event(|sample| {
        println!("Received: {}", sample.data);
    })?;
    pub_.publish(std_msgs::msg::String {
        data: "hello".into(),
    })?;
    std::thread::sleep(std::time::Duration::from_secs(1));
    Ok(())
}
