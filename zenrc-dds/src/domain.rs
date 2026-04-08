use std::ffi::CString;
use std::sync::Arc;

use crate::error::{check_entity, Result};
use crate::publisher::Publisher;
use crate::qos::Qos;
use crate::subscriber::Subscription;
use crate::topic::Topic;
use crate::msg_wrapper::RawMessageBridge;
use crate::{dds_domainid_t, dds_entity_t};

/// 让 CycloneDDS 自动选择域 ID（等同于 `DDS_DOMAIN_DEFAULT = UINT32_MAX`）
pub const DOMAIN_DEFAULT: u32 = u32::MAX;

// ─── 内部共享状态 ──────────────────────────────────────────────────────────────

pub(crate) struct ParticipantInner {
    pub entity: dds_entity_t,
}

impl Drop for ParticipantInner {
    fn drop(&mut self) {
        unsafe { crate::dds_delete(self.entity) };
    }
}

// SAFETY: dds_entity_t 是线程安全的 i32 句柄
unsafe impl Send for ParticipantInner {}
unsafe impl Sync for ParticipantInner {}

// ─── DomainParticipant ─────────────────────────────────────────────────────────

/// DDS 域参与者（Domain Participant），对应 ROS2 中的 `Node`。
///
/// 是创建 [`Publisher`] 和 [`Subscription`] 的工厂，内部使用 [`Arc`] 共享，
/// 保证在所有派生对象销毁之前不会删除底层 DDS 实体。
///
/// # 示例
/// ```ignore
/// use zenrc_dds::domain::{DomainParticipant, DOMAIN_DEFAULT};
/// use zenrc_dds::qos::Qos;
///
/// let dp = DomainParticipant::new(DOMAIN_DEFAULT).unwrap();
/// let publisher = dp.create_publisher::<MyMsg>("chatter", Qos::sensor_data()).unwrap();
/// ```
#[derive(Clone)]
pub struct DomainParticipant {
    pub(crate) inner: Arc<ParticipantInner>,
}

impl DomainParticipant {
    /// 使用默认 QoS 创建域参与者
    ///
    /// # 参数
    /// - `domain_id`：域 ID，使用 [`DOMAIN_DEFAULT`] 让系统自动选择
    pub fn new(domain_id: u32) -> Result<Self> {
        Self::new_with_qos(domain_id, None)
    }

    /// 使用指定 QoS 创建域参与者
    pub fn new_with_qos(domain_id: u32, qos: Option<&Qos>) -> Result<Self> {
        let qos_ptr = qos.map(|q| q.raw as *const _).unwrap_or(std::ptr::null());
        let entity = unsafe {
            crate::dds_create_participant(domain_id as dds_domainid_t, qos_ptr, std::ptr::null())
        };
        let entity = check_entity(entity)?;
        Ok(Self {
            inner: Arc::new(ParticipantInner { entity }),
        })
    }

    /// 获取域 ID
    pub fn domain_id(&self) -> Result<u32> {
        let mut id: dds_domainid_t = 0;
        crate::error::check_ret(unsafe {
            crate::dds_get_domainid(self.inner.entity, &mut id)
        })?;
        Ok(id)
    }

    // ── 创建 Topic ─────────────────────────────────────────────────────

    /// 创建带默认 QoS 的 Topic
    pub fn create_topic<T: RawMessageBridge>(&self, name: &str) -> Result<Topic<T>> {
        self.create_topic_with_qos(name, &Qos::default())
    }

    /// 创建带自定义 QoS 的 Topic
    pub fn create_topic_with_qos<T: RawMessageBridge>(
        &self,
        name: &str,
        qos: &Qos,
    ) -> Result<Topic<T>> {
        let c_name = CString::new(name)?;
        let entity = unsafe {
            crate::dds_create_topic(
                self.inner.entity,
                T::descriptor(),
                c_name.as_ptr(),
                qos.raw as *const _,
                std::ptr::null(),
            )
        };
        let entity = check_entity(entity)?;
        Ok(Topic::from_entity(entity))
    }

    // ── 创建 Publisher ─────────────────────────────────────────────────────

    /// 创建发布者（自动创建 Topic）
    ///
    /// # 参数
    /// - `topic_name`：Topic 名称
    /// - `qos`：QoS 策略，可使用 [`Qos`] 预设（如 [`Qos::sensor_data()`]）
    ///
    /// # 泛型参数
    /// - `T`：安全的 Rust 消息类型，必须实现 [`RawMessageBridge`]
    pub fn create_publisher<T: RawMessageBridge>(
        &self,
        topic_name: &str,
        qos: Qos,
    ) -> Result<Publisher<T>> {
        let topic = self.create_topic_with_qos::<T>(topic_name, &qos)?;
        let writer = unsafe {
            crate::dds_create_writer(
                self.inner.entity,
                topic.entity,
                qos.raw as *const _,
                std::ptr::null(),
            )
        };
        let writer = check_entity(writer)?;
        Ok(Publisher::new(writer, topic, Arc::clone(&self.inner)))
    }

    // ── 创建 Subscription ──────────────────────────────────────────────────

    /// 创建订阅者（自动创建 Topic）
    ///
    /// # 参数
    /// - `topic_name`：Topic 名称
    /// - `qos`：QoS 策略，需与发布者兼容
    ///
    /// # 泛型参数
    /// - `T`：安全的 Rust 消息类型，必须实现 [`RawMessageBridge`]
    pub fn create_subscription<T: RawMessageBridge>(
        &self,
        topic_name: &str,
        qos: Qos,
    ) -> Result<Subscription<T>> {
        let topic = self.create_topic_with_qos::<T>(topic_name, &qos)?;
        let reader = unsafe {
            crate::dds_create_reader(
                self.inner.entity,
                topic.entity,
                qos.raw as *const _,
                std::ptr::null(),
            )
        };
        let reader = check_entity(reader)?;
        Ok(Subscription::new(reader, topic, Arc::clone(&self.inner)))
    }

    // ── 辅助工具 ───────────────────────────────────────────────────────────

    /// 返回底层 DDS 参与者实体句柄（用于高级场景）
    pub fn entity(&self) -> dds_entity_t {
        self.inner.entity
    }

    /// 查找同域内的所有参与者实体
    pub fn lookup_participants(domain_id: u32) -> Result<Vec<dds_entity_t>> {
        const MAX: usize = 64;
        let mut buf = vec![0i32; MAX];
        let ret = unsafe {
            crate::dds_lookup_participant(domain_id as dds_domainid_t, buf.as_mut_ptr(), MAX)
        };
        let n = crate::error::check_entity(ret)? as usize;
        buf.truncate(n);
        Ok(buf)
    }
}
