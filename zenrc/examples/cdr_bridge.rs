//! CDR 序列化读写示例（网关/桥接模式）
//!
//! 演示 CDR 不反序列化直通转发流程：
//!
//! ```
//! [Publisher A] ---(typed write)---> [Topic: cdr_src]
//!                                         |
//!                            [Subscriber: take_cdr]  ← 不反序列化，直接取 CDR blob
//!                                         |
//!                            [Publisher B: forward_cdr] ← 不序列化，直接转发
//!                                         |
//!                                    [Topic: cdr_dst]
//!                                         |
//!                            [Subscriber: take] ← 正常反序列化读取
//! ```
//!
//! 运行:
//!   cargo run --example cdr_bridge -p zenrc

use std::time::Duration;

use zenrc::dds::context::DdsContext;
use zenrc::dds::qos::Qos;
use zenrc::msg::std_msgs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = DdsContext::new(0)?;

    // ── 拓扑创建 ─────────────────────────────────────────────────────────────
    // 源端：普通类型化发布者 + CDR 读取订阅者
    let pub_src = ctx.create_publisher::<std_msgs::msg::String>("cdr_src", Qos::sensor_data())?;
    let sub_cdr = ctx.create_subscriber::<std_msgs::msg::String>("cdr_src", Qos::sensor_data())?;

    // 目标端：CDR 转发发布者 + 普通类型化订阅者
    let pub_dst = ctx.create_publisher::<std_msgs::msg::String>("cdr_dst", Qos::sensor_data())?;
    let sub_dst = ctx.create_subscriber::<std_msgs::msg::String>("cdr_dst", Qos::sensor_data())?;

    // 等待发现完成
    tokio::time::sleep(Duration::from_millis(300)).await;

    // ── 步骤 1：写入若干条类型化消息到源 Topic ──────────────────────────────
    let count = 5usize;
    println!("=== 步骤1: 写入 {count} 条消息到 cdr_src ===");
    for i in 0..count {
        let msg = std_msgs::msg::String {
            data: format!("cdr bridge message #{i}"),
        };
        println!("  [pub_src] 写入: {}", msg.data);
        pub_src.publish(msg)?;
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    // ── 步骤 2：以 CDR 格式取出（不反序列化）──────────────────────────────
    println!("\n=== 步骤2: 从 cdr_src 取出 CDR blob（不反序列化） ===");
    let cdr_samples = sub_cdr.take_cdr(count)?;
    println!("  取得 {} 个 CDR blob", cdr_samples.len());
    for (i, sample) in cdr_samples.iter().enumerate() {
        println!(
            "  [cdr_blob #{i}] ts={} valid={}",
            sample.info.source_timestamp,
            sample.info.valid_data,
        );
    }

    // ── 步骤 3：将 CDR blob 转发到目标 Topic（不序列化）─────────────────────
    println!("\n=== 步骤3: 将 CDR blob 转发到 cdr_dst（不序列化） ===");
    for (i, sample) in cdr_samples.into_iter().enumerate() {
        println!("  [forward #{i}] 转发 ts={}", sample.info.source_timestamp);
        // forward_cdr 保留原始时间戳，适合网关/桥接场景
        pub_dst.forward_cdr(sample)?;
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    // ── 步骤 4：在目标端正常反序列化读取 ─────────────────────────────────
    println!("\n=== 步骤4: 从 cdr_dst 读取并反序列化 ===");
    let samples = sub_dst.take_cdr(count)?;
    // 这里用 take_cdr 展示 CDR 格式读取；也可 take() 进行类型化读取
    // 为与步骤2对比，同样打印 ts
    println!("  接收到 {} 个样本", samples.len());
    for (i, sample) in samples.iter().enumerate() {
        println!(
            "  [dst #{i}] ts={} valid={}",
            sample.info.source_timestamp,
            sample.info.valid_data,
        );
    }

    // ── 步骤 5：额外演示 peek_cdr + read_cdr 语义 ────────────────────────
    println!("\n=== 步骤5: peek_cdr / read_cdr 语义演示 ===");
    pub_src.publish(std_msgs::msg::String { data: "peek me".into() })?;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let peeked = sub_cdr.peek_cdr(1)?;
    println!("  peek_cdr 拿到 {} 条（消息仍在缓存）", peeked.len());
    let peeked2 = sub_cdr.peek_cdr(1)?;
    println!("  再次 peek_cdr 仍拿到 {} 条（消息未被消耗）", peeked2.len());

    let read = sub_cdr.read_cdr(1)?;
    println!("  read_cdr 拿到 {} 条（标记为已读，消息留在缓存）", read.len());

    let taken = sub_cdr.take_cdr(1)?;
    println!("  take_cdr 拿到 {} 条（消息从缓存移除）", taken.len());

    let empty = sub_cdr.take_cdr(1)?;
    println!("  再次 take_cdr: {} 条（缓存已空）", empty.len());

    println!("\n完成。");
    Ok(())
}
