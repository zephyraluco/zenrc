mod dds;

use std::time::Duration;
use dds::context::DdsContext;
use dds::qos::Qos;
use futures::StreamExt;
use zenrc_dds::std_msgs;

#[tokio::main]
async fn main() {
    // DdsContext 同时创建域参与者和后台 WaitSet 轮询线程
    let ctx = DdsContext::new(0).expect("创建 DDS 上下文失败");

    let publisher = ctx
        .create_publisher::<std_msgs::msg::String>("rt/test_string", Qos::sensor_data())
        .expect("创建发布者失败");
    let subscriber = ctx
        .create_subscription::<std_msgs::msg::String>("rt/test_string", Qos::sensor_data())
        .expect("创建订阅者失败");

    println!("Publisher/Dispatcher 已就绪，主题: 'rt/test_string'");

    // ── 订阅任务：将订阅者转为异步流，由调度器后台线程驱动 ─────────────────────
    tokio::spawn(async move {
        let mut stream = subscriber.into_stream(32);
        while let Some(result) = stream.next().await {
            match result {
                Ok(sample) => println!("收到: {:?}", sample.data),
                Err(e) => {
                    eprintln!("流错误: {e:?}");
                    break;
                }
            }
        }
    });

    // ── 发布循环 ─────────────────────────────────────────────────────────────
    let mut seq: u64 = 0;
    loop {
        let msg = std_msgs::msg::String {
            data: format!("hello #{seq}"),
        };
        if let Err(e) = publisher.publish(msg) {
            eprintln!("发布错误: {e:?}");
            break;
        }
        seq += 1;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

