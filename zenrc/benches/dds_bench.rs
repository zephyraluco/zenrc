use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use criterion::{Criterion, black_box, criterion_group, criterion_main};

#[path = "../src/dds/mod.rs"]
mod dds;

use dds::context::DdsContext;
use dds::qos::Qos;
use zenrc_dds::{RawMessageBridge, std_msgs};

static TOPIC_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_topic(prefix: &str) -> String {
    let pid = std::process::id();
    let seq = TOPIC_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{prefix}_{pid}_{seq}_{now_ns}")
}

fn wait_for_match(publisher: &dds::publisher::Publisher<std_msgs::msg::String>) {
    for _ in 0..200 {
        if publisher.has_readers().unwrap_or(false) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("writer did not match any reader within 2s");
}

fn drain_one(subscriber: &dds::subscriber::Subscription<std_msgs::msg::String>) {
    for _ in 0..20_000 {
        if let Some(sample) = subscriber
            .take_one()
            .expect("take_one should not fail while draining")
        {
            black_box(sample);
            return;
        }
        std::hint::spin_loop();
    }
    panic!("timed out waiting for one sample");
}

struct BenchIo {
    _ctx: DdsContext,
    publisher: dds::publisher::Publisher<std_msgs::msg::String>,
    subscriber: dds::subscriber::Subscription<std_msgs::msg::String>,
    rt: tokio::runtime::Runtime,
}

fn setup_bench_io(name: &str) -> BenchIo {
    let ctx = DdsContext::new(0).expect("create DdsContext");
    let topic = unique_topic(name);
    let pub_qos = Qos::system_default();
    let sub_qos = Qos::system_default();
    let publisher = ctx
        .create_publisher::<std_msgs::msg::String>(&topic, pub_qos)
        .expect("create publisher");
    let subscriber = ctx
        .create_subscription::<std_msgs::msg::String>(&topic, sub_qos)
        .expect("create subscription");

    wait_for_match(&publisher);

    publisher
        .publish(std_msgs::msg::String {
            data: String::from("warmup"),
        })
        .expect("warmup publish");
    drain_one(&subscriber);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("build tokio runtime");

    BenchIo {
        _ctx: ctx,
        publisher,
        subscriber,
        rt,
    }
}

fn bench_service_handler_like_path(c: &mut Criterion) {
    c.bench_function("service_handler_uppercase", |b| {
        b.iter(|| {
            let req = std_msgs::msg::String {
                data: String::from("hello benchmark"),
            };
            let res = std_msgs::msg::String {
                data: req.data.to_uppercase(),
            };
            black_box(res);
        });
    });
}

fn bench_message_bridge_roundtrip(c: &mut Criterion) {
    c.bench_function("std_msgs_string_to_raw_from_raw", |b| {
        b.iter(|| {
            let msg = std_msgs::msg::String {
                data: String::from("bridge roundtrip"),
            };
            let raw = black_box(msg).to_raw();
            let msg_back = std_msgs::msg::String::from_raw(raw);
            black_box(msg_back);
        });
    });
}

fn bench_notify_to_receive_latency(c: &mut Criterion) {
    let io = setup_bench_io("notify_to_receive_latency");

    c.bench_function("notify_to_receive_latency", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;

            for _ in 0..iters {
                let msg = std_msgs::msg::String {
                    data: String::from("latency sample"),
                };

                let t0 = Instant::now();
                io.publisher.publish(black_box(msg)).expect("publish in latency bench");
                let sample = io
                    .rt
                    .block_on(io.subscriber.next(Duration::from_secs(1)))
                    .expect("next in latency bench");
                total += t0.elapsed();
                black_box(sample);
            }

            total
        });
    });
}

fn bench_write_time(c: &mut Criterion) {
    let io = setup_bench_io("write_time");

    c.bench_function("write_time", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;

            for _ in 0..iters {
                let msg = std_msgs::msg::String {
                    data: String::from("write sample"),
                };

                let t0 = Instant::now();
                io.publisher.publish(black_box(msg)).expect("publish in write bench");
                total += t0.elapsed();

                drain_one(&io.subscriber);
            }

            total
        });
    });
}

fn bench_take_time(c: &mut Criterion) {
    let io = setup_bench_io("take_time");

    c.bench_function("take_time", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;

            for _ in 0..iters {
                io.publisher
                    .publish(std_msgs::msg::String {
                        data: String::from("take sample"),
                    })
                    .expect("publish in take bench");

                loop {
                    let t0 = Instant::now();
                    let sample = io.subscriber.take_one().expect("take_one in take bench");
                    match sample {
                        Some(s) => {
                            total += t0.elapsed();
                            black_box(s);
                            break;
                        }
                        None => std::hint::spin_loop(),
                    }
                }
            }

            total
        });
    });
}

criterion_group!(
    dds_benches,
    bench_service_handler_like_path,
    bench_message_bridge_roundtrip,
    bench_notify_to_receive_latency,
    bench_write_time,
    bench_take_time
);
criterion_main!(dds_benches);
