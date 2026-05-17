//! 零拷贝租借读取示例
//!
//! 演示 `read_wl` / `take_wl` 的使用：DDS 直接将内部缓冲区的指针借给我们，
//! 避免任何数据拷贝。每个 `LoanedSample` 离开作用域后自动调用 `dds_return_loan`
//! 归还租借内存。
//!
//! 运行:
//!   cargo run --example loan_read -p zenrc

use std::time::Duration;

use zenrc::dds::context::DdsContext;
use zenrc::dds::qos::Qos;
use zenrc::msg::LoanedSample;
use zenrc::msg::std_msgs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = DdsContext::new(0)?;

    let topic = "loan_demo";
    let publisher = ctx.create_publisher::<std_msgs::msg::String>(topic, Qos::sensor_data())?;
    let subscriber =
        ctx.create_subscriber::<std_msgs::msg::String>(topic, Qos::sensor_data())?;

    // 等待发现完成
    tokio::time::sleep(Duration::from_millis(200)).await;

    // ── 发布若干条消息 ───────────────────────────────────────────────────────
    for i in 0..1000u32 {
        let msg = std_msgs::msg::String {
            data: format!("loan message #{i}"),
        };
        println!("[pub] 发送: {}", msg.data);
        publisher.publish(msg)?;
    }

    // 短暂等待消息到达本地历史缓存
    tokio::time::sleep(Duration::from_millis(100)).await;

    // ── 零拷贝读取（read_wl）————消息仍留在历史缓存中 ───────────────────────
    {
        let loaned: Vec<LoanedSample<std_msgs::msg::String>> = subscriber.read_wl(10)?;
        println!("\n[read_wl] 读到 {} 条（租借，不拷贝）:", loaned.len());
        for sample in &loaned {
            if let Some(raw) = sample.get() {
                let data = if raw.data.is_null() {
                    "<null>".to_string()
                } else {
                    unsafe { std::ffi::CStr::from_ptr(raw.data) }
                        .to_string_lossy()
                        .into_owned()
                };
                println!("  ts={} data=\"{data}\"", sample.info.source_timestamp);
            }
        }
        // loaned 在此处 drop，每条 LoanedSample 各自归还租借内存
        println!("[read_wl] 租借已归还\n");
    }

    // ── 零拷贝取出（take_wl）————消息从历史缓存移除 ─────────────────────────
    {
        let loaned: Vec<LoanedSample<std_msgs::msg::String>> = subscriber.take_wl(10)?;
        println!("[take_wl] 取出 {} 条（租借，不拷贝）:", loaned.len());
        for sample in &loaned {
            if let Some(raw) = sample.get() {
                let data = if raw.data.is_null() {
                    "<null>".to_string()
                } else {
                    unsafe { std::ffi::CStr::from_ptr(raw.data) }
                        .to_string_lossy()
                        .into_owned()
                };
                println!("  ts={} data=\"{data}\"", sample.info.source_timestamp);
            }
        }
        // 每条 LoanedSample drop → dds_return_loan
        println!("[take_wl] 租借已归还\n");
    }

    // 验证 take_wl 后历史缓存为空
    {
        let loaned: Vec<LoanedSample<std_msgs::msg::String>> = subscriber.read_wl(10)?;
        println!("[验证] take 后再 read_wl 剩余: {} 条", loaned.len());
    }

    Ok(())
}
