mod dds;

use std::thread;
use std::time::Duration;

use dds::domain::DomainParticipant;
use dds::qos::Qos;
use zenrc_dds::std_msgs;

fn main() {
    let dp = DomainParticipant::new(0).expect("创建域参与者失败");

    let publisher = dp
        .create_publisher::<std_msgs::msg::String>("rt/test_string", Qos::sensor_data())
        .expect("创建发布者失败");
    let subscriber = dp
        .create_subscription::<std_msgs::msg::String>("rt/test_string", Qos::sensor_data())
        .expect("创建订阅者失败");

    println!("Publisher/Subcriber threads ready on topic 'rt/test_string'.");

    let pub_handle = thread::spawn(move || {
        let mut seq: u64 = 0;
        loop {
            let msg = std_msgs::msg::String {
                data: format!("hello #{seq}"),
            };
            if let Err(e) = publisher.publish(msg) {
                eprintln!("publish error: {e:?}");
                break;
            }
            seq += 1;
            thread::sleep(Duration::from_millis(100));
        }
    });

    let sub_handle = thread::spawn(move || {
        loop {
            match subscriber.wait_for_data(Duration::from_millis(500)) {
                Ok(true) => match subscriber.take_one() {
                    Ok(sample) => {
                        if let Some(sample) = sample {
                            println!("Received: {:?}", sample.data);
                        }
                    }
                    Err(e) => {
                        eprintln!("take error: {e:?}");
                        break;
                    }
                },
                Ok(false) => {}
                Err(e) => {
                    eprintln!("wait error: {e:?}");
                    break;
                }
            }
        }
    });

    let _ = pub_handle.join();
    let _ = sub_handle.join();
}
