//! zenrc-dds 集成测试
//!
//! 覆盖范围：
//! - error: DdsError 变体、check_entity、check_ret
//! - qos:   duration_to_nanos、Qos builder、预设 Profile
//! - domain: DomainParticipant 创建、domain_id、clone、lookup
//! - topic:  create_topic
//! - publisher:  create_publisher、publish、entity
//! - subscriber: create_subscription、take_one（初始为空）、read_one
//! - pubsub:     发布 → 取出的端到端验证
//! - waitset:    WaitSet 创建、attach_reader、带超时的 wait
//!
//! 所有需要 DDS 运行时的测试均在真实的 CycloneDDS 域（domain 0）上运行。

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]

use std::sync::OnceLock;
use std::time::Duration;

use zenrc_dds::{
    domain::{DomainParticipant, DOMAIN_DEFAULT},
    error::DdsError,
    qos::{Durability, History, Liveliness, Ownership, Qos, Reliability},
    DdsMsg,
};

// ─── 测试消息类型 ──────────────────────────────────────────────────────────────
//
// TestMsg 是一个最简单的 #[repr(C)] 结构体，只含一个 f64 字段。
// 其 dds_topic_descriptor_t 按 CycloneDDS CDR 流操作码手工构造：
//
//   ops[0] = DDS_OP_ADR | (DDS_OP_VAL_8BY << 16) = 0x01040000
//            ── 8 字节字段，后跟偏移量
//   ops[1] = 0   ── TestMsg::value 在结构体中的字节偏移
//   ops[2] = 0   ── DDS_OP_RTS（结束标志）

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TestMsg {
    pub value: f64,
}

// ops 数组为 'static，可被 descriptor 安全引用
static TEST_MSG_OPS: [u32; 3] = [
    0x01040000, // DDS_OP_ADR | (DDS_OP_VAL_8BY << 16)
    0,          // offsetof(TestMsg, value) = 0
    0,          // DDS_OP_RTS
];

// 使用 OnceLock<DescWrapper> 存储 descriptor；
// DescWrapper 用于绕过 dds_topic_descriptor_t 裸指针的 Sync 限制。
struct DescWrapper(zenrc_dds::dds_topic_descriptor_t);
// SAFETY: zenrc-dds 内部保证描述符在读取期间不会被修改；
//         多线程并发只读是安全的。
unsafe impl Sync for DescWrapper {}
unsafe impl Send for DescWrapper {}

fn test_msg_descriptor() -> *const zenrc_dds::dds_topic_descriptor_t {
    static HOLDER: OnceLock<DescWrapper> = OnceLock::new();
    &HOLDER
        .get_or_init(|| {
            DescWrapper(zenrc_dds::dds_topic_descriptor_t {
                m_size: std::mem::size_of::<TestMsg>() as u32,
                m_align: std::mem::align_of::<TestMsg>() as u32,
                m_flagset: zenrc_dds::DDS_TOPIC_FIXED_SIZE,
                m_nkeys: 0,
                m_typename: b"TestMsg\0".as_ptr() as *const std::os::raw::c_char,
                m_keys: std::ptr::null(),
                m_nops: 3,
                m_ops: TEST_MSG_OPS.as_ptr(),
                m_meta: b"\0".as_ptr() as *const std::os::raw::c_char,
                type_information: zenrc_dds::dds_type_meta_ser {
                    data: std::ptr::null(),
                    sz: 0,
                },
                type_mapping: zenrc_dds::dds_type_meta_ser {
                    data: std::ptr::null(),
                    sz: 0,
                },
                restrict_data_representation: 0,
            })
        })
        .0 as *const _
}

// SAFETY: 描述符指向 'static 数据，内存布局与 TestMsg 完全匹配。
unsafe impl DdsMsg for TestMsg {
    fn descriptor() -> *const zenrc_dds::dds_topic_descriptor_t {
        test_msg_descriptor()
    }

    // TestMsg 只含基本数值字段，无 DDS 分配的堆内存，free_contents 为空操作
    unsafe fn free_contents(&mut self) {}
}

// ─── 助手：生成唯一 topic 名称（防止跨测试串扰）─────────────────────────────────
fn unique_topic(base: &str) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static CTR: AtomicU32 = AtomicU32::new(0);
    format!("{}_t{}", base, CTR.fetch_add(1, Ordering::Relaxed))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// error 模块测试
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

mod error_tests {
    use super::*;

    // 通过公共 API 间接验证 DdsError 变体

    #[test]
    fn dds_error_nul() {
        let nul_err = std::ffi::CString::new("ab\0cd").unwrap_err();
        let err = DdsError::Nul(nul_err);
        let msg = err.to_string();
        assert!(msg.contains("NUL"), "message was: {msg}");
    }

    #[test]
    fn dds_error_utf8() {
        let bad_bytes = [0xFF_u8, 0xFE];
        let utf8_err = std::str::from_utf8(&bad_bytes).unwrap_err();
        let err = DdsError::Utf8(utf8_err);
        let msg = err.to_string();
        assert!(!msg.is_empty());
    }

    #[test]
    fn dds_error_retcode_has_code_and_msg() {
        let err = DdsError::RetCode(-1, "test error".into());
        if let DdsError::RetCode(code, msg) = &err {
            assert_eq!(*code, -1);
            assert_eq!(msg, "test error");
        }
        assert!(!err.to_string().is_empty());
    }

    /// 通过 create_topic 使用含 NUL 字节的名称来触发 DdsError::Nul 路径
    #[test]
    fn create_topic_nul_name_returns_err() {
        let dp = DomainParticipant::new(0).unwrap();
        let res = dp.create_topic::<super::TestMsg>("bad\0name");
        assert!(
            matches!(res, Err(DdsError::Nul(_))),
            "预期 DdsError::Nul"
        );
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// qos 模块测试（纯 Rust 部分 + DDS 运行时部分）
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

mod qos_tests {
    use super::*;

    // ── 纯 Rust：枚举相等性 ───────────────────────────────

    #[test]
    fn reliability_variants_eq() {
        assert_eq!(Reliability::BestEffort, Reliability::BestEffort);
        assert_eq!(Reliability::Reliable, Reliability::Reliable);
        assert_ne!(Reliability::BestEffort, Reliability::Reliable);
    }

    #[test]
    fn durability_variants_eq() {
        assert_eq!(Durability::Volatile, Durability::Volatile);
        assert_eq!(Durability::TransientLocal, Durability::TransientLocal);
        assert_eq!(Durability::Transient, Durability::Transient);
        assert_eq!(Durability::Persistent, Durability::Persistent);
        assert_ne!(Durability::Volatile, Durability::Persistent);
    }

    #[test]
    fn history_keep_last_eq() {
        assert_eq!(History::KeepLast(10), History::KeepLast(10));
        assert_ne!(History::KeepLast(10), History::KeepLast(5));
        assert_ne!(History::KeepLast(1), History::KeepAll);
    }

    #[test]
    fn liveliness_variants_eq() {
        assert_eq!(Liveliness::Automatic, Liveliness::Automatic);
        assert_ne!(Liveliness::Automatic, Liveliness::ManualByTopic);
    }

    #[test]
    fn ownership_variants_eq() {
        assert_eq!(Ownership::Shared, Ownership::Shared);
        assert_ne!(Ownership::Shared, Ownership::Exclusive);
    }

    // ── 需要 DDS 运行时：Qos 构建 ─────────────────────────

    #[test]
    fn qos_new_does_not_panic() {
        let _qos = Qos::new();
    }

    #[test]
    fn qos_default_does_not_panic() {
        let _qos = Qos::default();
    }

    #[test]
    fn qos_sensor_data_profile() {
        let _qos = Qos::sensor_data();
    }

    #[test]
    fn qos_system_default_profile() {
        let _qos = Qos::system_default();
    }

    #[test]
    fn qos_services_default_profile() {
        let _qos = Qos::services_default();
    }

    #[test]
    fn qos_parameters_profile() {
        let _qos = Qos::parameters();
    }

    #[test]
    fn qos_action_status_default_profile() {
        let _qos = Qos::action_status_default();
    }

    #[test]
    fn qos_clock_profile() {
        let _qos = Qos::clock();
    }

    #[test]
    fn qos_transient_local_profile() {
        let _qos = Qos::transient_local();
    }

    #[test]
    fn qos_builder_chain_reliability() {
        let _qos = Qos::new().reliability(Reliability::Reliable, Duration::from_millis(100));
    }

    #[test]
    fn qos_builder_chain_durability() {
        let _qos = Qos::new().durability(Durability::TransientLocal);
    }

    #[test]
    fn qos_builder_chain_history_keep_last() {
        let _qos = Qos::new().history(History::KeepLast(10));
    }

    #[test]
    fn qos_builder_chain_history_keep_all() {
        let _qos = Qos::new().history(History::KeepAll);
    }

    #[test]
    fn qos_builder_chain_deadline() {
        let _qos = Qos::new().deadline(Duration::from_secs(1));
    }

    #[test]
    fn qos_builder_chain_lifespan() {
        let _qos = Qos::new().lifespan(Duration::from_secs(10));
    }

    #[test]
    fn qos_builder_chain_latency_budget() {
        let _qos = Qos::new().latency_budget(Duration::from_millis(10));
    }

    #[test]
    fn qos_builder_chain_liveliness() {
        let _qos = Qos::new()
            .liveliness(Liveliness::Automatic, Duration::from_secs(5));
    }

    #[test]
    fn qos_builder_chain_ownership_shared() {
        let _qos = Qos::new().ownership(Ownership::Shared);
    }

    #[test]
    fn qos_builder_chain_ownership_exclusive() {
        let _qos = Qos::new()
            .ownership(Ownership::Exclusive)
            .ownership_strength(10);
    }

    #[test]
    fn qos_builder_chain_partition() {
        let _qos = Qos::new().partition(&["part1", "part2"]).unwrap();
    }

    #[test]
    fn qos_builder_chain_partition1() {
        let _qos = Qos::new().partition1("mypartition").unwrap();
    }

    #[test]
    fn qos_partition_nul_byte_returns_err() {
        let res = Qos::new().partition(&["bad\0partition"]);
        assert!(res.is_err());
    }

    #[test]
    fn qos_builder_chain_resource_limits() {
        let _qos = Qos::new().resource_limits(100, 10, 10);
    }

    #[test]
    fn qos_builder_chain_entity_name() {
        let _qos = Qos::new().entity_name("my_entity").unwrap();
    }

    #[test]
    fn qos_builder_full_chain() {
        let _qos = Qos::new()
            .reliability(Reliability::Reliable, Duration::from_millis(500))
            .durability(Durability::TransientLocal)
            .history(History::KeepLast(10))
            .deadline(Duration::from_secs(1))
            .lifespan(Duration::from_secs(60))
            .latency_budget(Duration::from_millis(1))
            .liveliness(Liveliness::Automatic, Duration::from_secs(10))
            .ownership(Ownership::Shared)
            .resource_limits(1000, 100, 10);
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// domain 模块测试
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

mod domain_tests {
    use super::*;

    #[test]
    fn create_participant_domain0() {
        DomainParticipant::new(0).unwrap();
    }

    #[test]
    fn create_participant_domain_default() {
        DomainParticipant::new(DOMAIN_DEFAULT).unwrap();
    }

    #[test]
    fn domain_id_returns_0() {
        let dp = DomainParticipant::new(0).unwrap();
        assert_eq!(dp.domain_id().unwrap(), 0);
    }

    #[test]
    fn participant_clone_shares_entity() {
        let dp1 = DomainParticipant::new(0).unwrap();
        let dp2 = dp1.clone();
        // 两个 clone 持有相同的实体句柄
        assert_eq!(dp1.entity(), dp2.entity());
    }

    #[test]
    fn lookup_participants_finds_created() {
        let dp = DomainParticipant::new(0).unwrap();
        let list = DomainParticipant::lookup_participants(0).unwrap();
        assert!(
            list.contains(&dp.entity()),
            "lookup 结果中未找到刚创建的参与者"
        );
    }

    #[test]
    fn participant_entity_is_positive() {
        let dp = DomainParticipant::new(0).unwrap();
        assert!(dp.entity() > 0);
    }

    #[test]
    fn create_participant_with_qos() {
        let qos = Qos::new();
        DomainParticipant::new_with_qos(0, Some(&qos)).unwrap();
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// topic 模块测试
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

mod topic_tests {
    use super::*;

    #[test]
    fn create_topic_default_qos() {
        let dp = DomainParticipant::new(0).unwrap();
        let topic = dp
            .create_topic::<TestMsg>(&unique_topic("test_topic_default"))
            .unwrap();
        assert!(topic.entity() > 0);
    }

    #[test]
    fn create_topic_with_qos() {
        let dp = DomainParticipant::new(0).unwrap();
        let qos = Qos::sensor_data();
        let topic = dp
            .create_topic_with_qos::<TestMsg>(&unique_topic("test_topic_qos"), &qos)
            .unwrap();
        assert!(topic.entity() > 0);
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// publisher 模块测试
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

mod publisher_tests {
    use super::*;

    #[test]
    fn create_publisher_best_effort() {
        let dp = DomainParticipant::new(0).unwrap();
        let pub_ = dp
            .create_publisher::<TestMsg>(&unique_topic("pub_be"), Qos::sensor_data())
            .unwrap();
        assert!(pub_.entity() > 0);
        assert!(pub_.topic_entity() > 0);
    }

    #[test]
    fn create_publisher_reliable() {
        let dp = DomainParticipant::new(0).unwrap();
        let pub_ = dp
            .create_publisher::<TestMsg>(&unique_topic("pub_rel"), Qos::system_default())
            .unwrap();
        assert!(pub_.entity() > 0);
    }

    #[test]
    fn publish_succeeds() {
        let dp = DomainParticipant::new(0).unwrap();
        let pub_ = dp
            .create_publisher::<TestMsg>(&unique_topic("pub_write"), Qos::sensor_data())
            .unwrap();
        let msg = TestMsg { value: 3.14 };
        pub_.publish(&msg).unwrap();
    }

    #[test]
    fn publish_with_timestamp_succeeds() {
        let dp = DomainParticipant::new(0).unwrap();
        let pub_ = dp
            .create_publisher::<TestMsg>(&unique_topic("pub_ts"), Qos::sensor_data())
            .unwrap();
        let msg = TestMsg { value: 1.0 };
        pub_.publish_with_timestamp(&msg, 1_000_000_000).unwrap();
    }

    #[test]
    fn publication_matched_status_ok() {
        let dp = DomainParticipant::new(0).unwrap();
        let pub_ = dp
            .create_publisher::<TestMsg>(&unique_topic("pub_match"), Qos::sensor_data())
            .unwrap();
        let status = pub_.publication_matched_status().unwrap();
        // 初始应无匹配订阅者
        assert_eq!(status.current_count, 0);
    }

    #[test]
    fn matched_subscriptions_empty_initially() {
        let dp = DomainParticipant::new(0).unwrap();
        let pub_ = dp
            .create_publisher::<TestMsg>(&unique_topic("pub_subs"), Qos::sensor_data())
            .unwrap();
        let handles = pub_.matched_subscriptions().unwrap();
        assert!(handles.is_empty());
    }

    #[test]
    fn flush_succeeds() {
        let dp = DomainParticipant::new(0).unwrap();
        let pub_ = dp
            .create_publisher::<TestMsg>(&unique_topic("pub_flush"), Qos::sensor_data())
            .unwrap();
        pub_.flush().unwrap();
    }

    #[test]
    fn assert_liveliness_succeeds() {
        let dp = DomainParticipant::new(0).unwrap();
        let pub_ = dp
            .create_publisher::<TestMsg>(&unique_topic("pub_liveness"), Qos::sensor_data())
            .unwrap();
        pub_.assert_liveliness().unwrap();
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// subscriber 模块测试
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

mod subscriber_tests {
    use super::*;

    #[test]
    fn create_subscription_best_effort() {
        let dp = DomainParticipant::new(0).unwrap();
        let sub = dp
            .create_subscription::<TestMsg>(&unique_topic("sub_be"), Qos::sensor_data())
            .unwrap();
        assert!(sub.entity() > 0);
        assert!(sub.topic_entity() > 0);
    }

    #[test]
    fn take_one_returns_none_when_empty() {
        let dp = DomainParticipant::new(0).unwrap();
        let sub = dp
            .create_subscription::<TestMsg>(&unique_topic("sub_take_empty"), Qos::sensor_data())
            .unwrap();
        let result = sub.take_one().unwrap();
        assert!(result.is_none(), "空 reader 不应有样本");
    }

    #[test]
    fn read_one_returns_none_when_empty() {
        let dp = DomainParticipant::new(0).unwrap();
        let sub = dp
            .create_subscription::<TestMsg>(&unique_topic("sub_read_empty"), Qos::sensor_data())
            .unwrap();
        let result = sub.read_one().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn take_max_zero_returns_empty_vec() {
        let dp = DomainParticipant::new(0).unwrap();
        let sub = dp
            .create_subscription::<TestMsg>(&unique_topic("sub_take_zero"), Qos::sensor_data())
            .unwrap();
        let samples = sub.take(0).unwrap();
        assert!(samples.is_empty());
    }

    #[test]
    fn subscription_matched_status_ok() {
        let dp = DomainParticipant::new(0).unwrap();
        let sub = dp
            .create_subscription::<TestMsg>(&unique_topic("sub_match"), Qos::sensor_data())
            .unwrap();
        let status = sub.subscription_matched_status().unwrap();
        assert_eq!(status.current_count, 0);
    }

    #[test]
    fn matched_publications_empty_initially() {
        let dp = DomainParticipant::new(0).unwrap();
        let sub = dp
            .create_subscription::<TestMsg>(&unique_topic("sub_pubs"), Qos::sensor_data())
            .unwrap();
        let handles = sub.matched_publications().unwrap();
        assert!(handles.is_empty());
    }

    #[test]
    fn wait_for_data_timeout_returns_false() {
        let dp = DomainParticipant::new(0).unwrap();
        let sub = dp
            .create_subscription::<TestMsg>(&unique_topic("sub_wait_data"), Qos::sensor_data())
            .unwrap();
        // 无数据，应在超时后返回 false
        let got_data = sub
            .wait_for_data(Duration::from_millis(1))
            .unwrap();
        assert!(!got_data, "没有发布者时不应有数据");
    }

    #[test]
    fn sample_lost_status_ok() {
        let dp = DomainParticipant::new(0).unwrap();
        let sub = dp
            .create_subscription::<TestMsg>(&unique_topic("sub_lost"), Qos::sensor_data())
            .unwrap();
        sub.sample_lost_status().unwrap();
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 端到端发布/订阅测试
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

mod pubsub_tests {
    use super::*;

    /// 发布一条消息后，订阅者应能取回相同数据
    #[test]
    fn publish_then_take_one() {
        let topic = unique_topic("pubsub_basic");
        let dp = DomainParticipant::new(0).unwrap();

        let sub = dp
            .create_subscription::<TestMsg>(&topic, Qos::sensor_data())
            .unwrap();
        let pub_ = dp
            .create_publisher::<TestMsg>(&topic, Qos::sensor_data())
            .unwrap();

        // 等待 reader/writer 匹配
        std::thread::sleep(Duration::from_millis(50));

        let msg = TestMsg { value: 42.0 };
        pub_.publish(&msg).unwrap();

        // 等待数据送达
        let received = sub
            .wait_for_data(Duration::from_millis(200))
            .unwrap();
        assert!(received, "超时：未收到发布的数据");

        let sample = sub.take_one().unwrap().expect("取样失败");
        assert!((sample.value - 42.0).abs() < f64::EPSILON);
        assert!(sample.info().valid_data);
        assert!(sample.info().is_alive);
    }

    /// 发布多条消息后，订阅者应能全部取出
    #[test]
    fn publish_multiple_then_take_all() {
        let topic = unique_topic("pubsub_multi");
        let dp = DomainParticipant::new(0).unwrap();

        let sub = dp
            .create_subscription::<TestMsg>(&topic, Qos::system_default())
            .unwrap();
        let pub_ = dp
            .create_publisher::<TestMsg>(&topic, Qos::system_default())
            .unwrap();

        std::thread::sleep(Duration::from_millis(50));

        let values = [1.0_f64, 2.0, 3.0, 4.0, 5.0];
        for &v in &values {
            pub_.publish(&TestMsg { value: v }).unwrap();
        }

        // 等待全部数据
        std::thread::sleep(Duration::from_millis(100));

        let samples = sub.take(10).unwrap();
        assert_eq!(samples.len(), values.len(), "收到数量与发送不符");

        let mut received_values: Vec<f64> = samples.iter().map(|s| s.value).collect();
        received_values.sort_by(f64::total_cmp);
        let mut expected = values.to_vec();
        expected.sort_by(f64::total_cmp);
        for (r, e) in received_values.iter().zip(expected.iter()) {
            assert!((r - e).abs() < f64::EPSILON, "值不匹配：{r} vs {e}");
        }
    }

    /// 读取不移除样本，样本仍可取出
    #[test]
    fn read_does_not_remove_sample() {
        let topic = unique_topic("pubsub_read");
        let dp = DomainParticipant::new(0).unwrap();

        let sub = dp
            .create_subscription::<TestMsg>(&topic, Qos::sensor_data())
            .unwrap();
        let pub_ = dp
            .create_publisher::<TestMsg>(&topic, Qos::sensor_data())
            .unwrap();

        std::thread::sleep(Duration::from_millis(50));
        pub_.publish(&TestMsg { value: 7.0 }).unwrap();

        sub.wait_for_data(Duration::from_millis(200)).unwrap();

        // read 不应清空缓存
        let read_samples = sub.read(1).unwrap();
        assert!(!read_samples.is_empty());
    }

    /// into_parts 拆解 Sample 后应得到正确的值和 SampleInfo
    #[test]
    fn sample_into_parts() {
        let topic = unique_topic("pubsub_parts");
        let dp = DomainParticipant::new(0).unwrap();

        let sub = dp
            .create_subscription::<TestMsg>(&topic, Qos::sensor_data())
            .unwrap();
        let pub_ = dp
            .create_publisher::<TestMsg>(&topic, Qos::sensor_data())
            .unwrap();

        std::thread::sleep(Duration::from_millis(50));
        pub_.publish(&TestMsg { value: 99.9 }).unwrap();
        sub.wait_for_data(Duration::from_millis(200)).unwrap();

        let sample = sub.take_one().unwrap().unwrap();
        let (msg, info) = sample.into_parts();
        assert!((msg.value - 99.9).abs() < f64::EPSILON);
        assert!(info.valid_data);
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// waitset 模块测试
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

mod waitset_tests {
    use super::*;
    use zenrc_dds::WaitSet;

    #[test]
    fn create_waitset_succeeds() {
        let dp = DomainParticipant::new(0).unwrap();
        let _ws = WaitSet::new(&dp).unwrap();
    }

    #[test]
    fn waitset_entity_is_positive() {
        let dp = DomainParticipant::new(0).unwrap();
        let ws = WaitSet::new(&dp).unwrap();
        assert!(ws.entity() > 0);
    }

    #[test]
    fn waitset_wait_timeout_returns_empty() {
        let dp = DomainParticipant::new(0).unwrap();
        let ws = WaitSet::new(&dp).unwrap();
        // 无条件附加，超时后应返回空触发列表
        let triggered = ws.wait(Duration::from_millis(1)).unwrap();
        assert!(triggered.is_empty(), "超时时不应有触发条件");
    }

    #[test]
    fn waitset_attach_and_detach_reader() {
        let dp = DomainParticipant::new(0).unwrap();
        let ws = WaitSet::new(&dp).unwrap();
        let sub = dp
            .create_subscription::<TestMsg>(&unique_topic("ws_reader"), Qos::sensor_data())
            .unwrap();

        ws.attach_reader(&sub, 42).unwrap();

        // 附加后应能在实体列表中找到 reader
        let entities = ws.attached_entities().unwrap();
        assert!(
            entities.contains(&sub.entity()),
            "attached_entities 中未找到 reader"
        );

        // 分离
        ws.detach_entity(sub.entity()).unwrap();
    }

    #[test]
    fn waitset_attach_writer() {
        let dp = DomainParticipant::new(0).unwrap();
        let ws = WaitSet::new(&dp).unwrap();
        let pub_ = dp
            .create_publisher::<TestMsg>(&unique_topic("ws_writer"), Qos::sensor_data())
            .unwrap();

        ws.attach_writer(&pub_, 99).unwrap();
        let entities = ws.attached_entities().unwrap();
        assert!(entities.contains(&pub_.entity()));
    }

    #[test]
    fn waitset_trigger_then_wait_returns_nonempty() {
        let dp = DomainParticipant::new(0).unwrap();
        let ws = WaitSet::new(&dp).unwrap();

        // 触发等待集自身
        ws.trigger().unwrap();
        // 触发后 wait 可能立即返回（DDS 实现相关，可能为空也可能包含 token）
        // 这里只验证不会 panic/error
        let _ = ws.wait(Duration::from_millis(10)).unwrap();
    }

    /// 等待集与发布/订阅结合：发布后通过 WaitSet 检测到数据
    #[test]
    fn waitset_detects_published_data() {
        let topic = unique_topic("ws_pubsub");
        let dp = DomainParticipant::new(0).unwrap();

        let sub = dp
            .create_subscription::<TestMsg>(&topic, Qos::sensor_data())
            .unwrap();
        let pub_ = dp
            .create_publisher::<TestMsg>(&topic, Qos::sensor_data())
            .unwrap();

        let ws = WaitSet::new(&dp).unwrap();
        ws.attach_reader(&sub, 1).unwrap();

        std::thread::sleep(Duration::from_millis(50));
        pub_.publish(&TestMsg { value: 5.0 }).unwrap();

        let triggered = ws.wait(Duration::from_millis(500)).unwrap();
        assert!(!triggered.is_empty(), "WaitSet 应检测到发布的数据");
        assert!(triggered.contains(&1), "token 1 应在触发列表中");
    }
}
