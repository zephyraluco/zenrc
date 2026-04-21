use std::collections::HashMap;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use zenrc_dds::{dds_attach_t, dds_domainid_t, dds_entity_t, RawMessageBridge, DDS_ANY_STATE};

use super::error::{check_entity, check_ret, DdsError, Result};
use super::publisher::Publisher;
use super::qos::Qos;
use super::subscriber::Subscription;
use super::topic::Topic;

// ─── 常量 ─────────────────────────────────────────────────────────────────────

/// 让 CycloneDDS 自动选择域 ID（等同于 `DDS_DOMAIN_DEFAULT = UINT32_MAX`）
pub const DOMAIN_DEFAULT: u32 = u32::MAX;

// ─── DomainParticipant ────────────────────────────────────────────────────────

/// DDS 域参与者（Domain Participant），是创建 Publisher 和 Subscription 的工厂。
///
/// 内部使用 [`Arc`] 共享，保证在所有派生对象销毁之前不会删除底层 DDS 实体。
/// 通常通过 [`DdsContext::new`] 隐式创建，可通过 `ctx.participant` 访问。
#[derive(Clone)]
pub struct DomainParticipant {
    entity: dds_entity_t,
}

impl DomainParticipant {
    /// 使用默认 QoS 创建域参与者
    pub fn new(domain_id: u32) -> Result<Self> {
        Self::new_with_qos(domain_id, None)
    }

    /// 使用指定 QoS 创建域参与者
    pub fn new_with_qos(domain_id: u32, qos: Option<&Qos>) -> Result<Self> {
        let qos_ptr = qos.map(|q| q.raw as *const _).unwrap_or(std::ptr::null());
        let entity = unsafe {
            zenrc_dds::dds_create_participant(domain_id as dds_domainid_t, qos_ptr, std::ptr::null())
        };
        let entity = check_entity(entity)?;
        Ok(Self {
            entity
        })
    }

    /// 获取域 ID
    pub fn domain_id(&self) -> Result<u32> {
        let mut id: dds_domainid_t = 0;
        super::error::check_ret(unsafe {
            zenrc_dds::dds_get_domainid(self.entity, &mut id)
        })?;
        Ok(id)
    }

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
            zenrc_dds::dds_create_topic(
                self.entity,
                T::descriptor(),
                c_name.as_ptr(),
                qos.raw as *const _,
                std::ptr::null(),
            )
        };
        let entity = check_entity(entity)?;
        Ok(Topic::from_entity(entity))
    }

    /// 创建发布者（自动创建 Topic，不附加到任何 WaitSet）
    pub fn create_publisher<T: RawMessageBridge>(
        &self,
        topic_name: &str,
        qos: Qos,
    ) -> Result<Publisher<T>> {
        let topic = self.create_topic_with_qos::<T>(topic_name, &qos)?;
        let writer = unsafe {
            zenrc_dds::dds_create_writer(
                self.entity,
                topic.entity,
                qos.raw as *const _,
                std::ptr::null(),
            )
        };
        let writer = check_entity(writer)?;
        Ok(Publisher::new(writer, topic))
    }

    /// 创建订阅者（自动创建 Topic，不附加到任何 WaitSet）
    pub fn create_subscription<T: RawMessageBridge>(
        &self,
        topic_name: &str,
        qos: Qos,
    ) -> Result<Subscription<T>> {
        let topic = self.create_topic_with_qos::<T>(topic_name, &qos)?;
        let sub = unsafe {
            zenrc_dds::dds_create_subscriber(
                self.entity,
                qos.raw as *const _,
                std::ptr::null(),
            )
        };
        let reader = unsafe {
            zenrc_dds::dds_create_reader(
                sub,
                topic.entity,
                qos.raw as *const _,
                std::ptr::null(),
            )
        };
        let reader = check_entity(reader)?;
        Ok(Subscription::new(reader, topic))
    }

    /// 返回底层 DDS 参与者实体句柄
    pub fn entity(&self) -> dds_entity_t {
        self.entity
    }

    /// 查找同域内的所有参与者实体
    pub fn lookup_participants(domain_id: u32) -> Result<Vec<dds_entity_t>> {
        const MAX: usize = 64;
        let mut buf = vec![0i32; MAX];
        let ret = unsafe {
            zenrc_dds::dds_lookup_participant(domain_id as dds_domainid_t, buf.as_mut_ptr(), MAX)
        };
        let n = super::error::check_entity(ret)? as usize;
        buf.truncate(n);
        Ok(buf)
    }
}

// ─── 内部常量 ──────────────────────────────────────────────────────────────────

/// 守护条件 attach token，用于唤醒/停止后台线程
const WAKE_TOKEN: dds_attach_t = 0;

/// 每次 dds_waitset_wait 的超时（100 ms），兜底检测 running 标志
const POLL_TIMEOUT_NS: i64 = 100_000_000;

/// WaitSet 单次最大触发条件数
const MAX_TRIGGERS: usize = 64;

// ─── 内部结构 ──────────────────────────────────────────────────────────────────

struct ReaderEntry {
    /// 已附加到 WaitSet 的 ReadCondition 实体句柄
    readcond: dds_entity_t,
    /// 数据到达时唤醒等待方的通知句柄
    #[cfg(feature = "async")]
    notify: Arc<tokio::sync::Notify>,
}

pub(crate) struct ContextCore {
    waitset: dds_entity_t,
    guard: dds_entity_t,
    running: AtomicBool,
    /// token（reader entity as isize）→ ReaderEntry
    readers: Mutex<HashMap<isize, ReaderEntry>>,
}

// SAFETY: dds_entity_t 是 i32 句柄，CycloneDDS 所有操作均线程安全
unsafe impl Send for ContextCore {}
unsafe impl Sync for ContextCore {}

impl Drop for ContextCore {
    fn drop(&mut self) {
        // 后台线程已由 DdsContext::drop 中的 join() 确认退出，可安全清理 DDS 资源
        if let Ok(readers) = self.readers.lock() {
            for entry in readers.values() {
                unsafe { zenrc_dds::dds_delete(entry.readcond) };
            }
        }
        unsafe { zenrc_dds::dds_delete(self.waitset) };
        unsafe { zenrc_dds::dds_delete(self.guard) };
    }
}

impl ContextCore {
    /// 将 reader 的 ReadCondition 附加到 WaitSet，返回数据到达通知句柄。
    ///
    /// 由 [`DdsContext::create_subscription`] 在构造 [`Subscription`] 时自动调用。
    #[cfg(feature = "async")]
    pub(crate) fn attach(&self, reader: dds_entity_t) -> Option<Arc<tokio::sync::Notify>> {
        let readcond = check_entity(unsafe {
            zenrc_dds::dds_create_readcondition(reader, DDS_ANY_STATE)
        })
        .ok()?;

        let notify = Arc::new(tokio::sync::Notify::new());
        let token = reader as isize;

        {
            let mut readers = self.readers.lock().unwrap();
            readers.insert(
                token,
                ReaderEntry {
                    readcond,
                    notify: Arc::clone(&notify),
                },
            );
        }

        if check_ret(unsafe {
            zenrc_dds::dds_waitset_attach(self.waitset, readcond, token)
        })
        .is_err()
        {
            self.readers.lock().unwrap().remove(&token);
            unsafe { zenrc_dds::dds_delete(readcond) };
            return None;
        }

        // 唤醒后台线程以感知新附加的 ReadCondition
        unsafe { zenrc_dds::dds_set_guardcondition(self.guard, true) };

        Some(notify)
    }

    /// 从 WaitSet 移除 reader 的 ReadCondition。
    ///
    /// 由 [`Subscription::drop`](super::subscriber::Subscription) 自动调用。
    #[cfg(feature = "async")]
    pub(crate) fn detach(&self, reader: dds_entity_t) {
        let token = reader as isize;
        let entry = self.readers.lock().unwrap().remove(&token);
        if let Some(entry) = entry {
            unsafe { zenrc_dds::dds_waitset_detach(self.waitset, entry.readcond) };
            unsafe { zenrc_dds::dds_delete(entry.readcond) };
        }
    }
}

// ─── DdsContext ────────────────────────────────────────────────────────────────

/// DDS 上下文（RAII），包含 [`DomainParticipant`] 和后台 WaitSet 轮询线程。
///
/// 调用 [`DdsContext::new`] 同时创建域参与者和共享 WaitSet。
/// 通过 [`DdsContext::create_subscription`] 创建的订阅者会自动将 ReadCondition
/// 附加到本实例的 WaitSet，数据到达时后台线程通过 [`tokio::sync::Notify`]
/// 唤醒等待中的异步 API。
///
/// `DdsContext` 被 drop 时，后台线程优雅退出（设置 `running = false` → 触发守护
/// 条件 → `join`）。
///
/// # 示例
/// ```ignore
/// use crate::dds::context::DdsContext;
/// use crate::dds::qos::Qos;
///
/// let ctx = DdsContext::new(0)?;
/// let publisher = ctx.create_publisher::<MyMsg>("chatter", Qos::sensor_data())?;
/// let subscriber = ctx.create_subscription::<MyMsg>("chatter", Qos::sensor_data())?;
/// ```
pub struct DdsContext {
    /// 域参与者；也可直接用于同步操作（创建的 Subscription 不会附加到 WaitSet）
    pub participant: DomainParticipant,
    pub(crate) core: Arc<ContextCore>,
    thread: Option<thread::JoinHandle<()>>,
}

// SAFETY: DomainParticipant（Arc<ParticipantInner>）Send+Sync；
//         Arc<ContextCore> Send+Sync；thread 只在 drop(&mut self) 中访问
unsafe impl Send for DdsContext {}
unsafe impl Sync for DdsContext {}

impl DdsContext {
    /// 使用默认 QoS 创建上下文，同时创建域参与者和后台 WaitSet 轮询线程
    ///
    /// # 参数
    /// - `domain_id`：域 ID，使用 [`super::domain::DOMAIN_DEFAULT`] 让系统自动选择
    pub fn new(domain_id: u32) -> Result<Self> {
        Self::new_with_qos(domain_id, None)
    }

    /// 使用指定 QoS 创建上下文
    pub fn new_with_qos(domain_id: u32, qos: Option<&Qos>) -> Result<Self> {
        let participant = DomainParticipant::new_with_qos(domain_id, qos)?;
        let participant_entity = participant.entity();

        // 创建 WaitSet
        let ws = check_entity(unsafe { zenrc_dds::dds_create_waitset(participant_entity) })?;

        // 创建守护条件（用于唤醒/停止后台线程）
        let guard = match check_entity(unsafe {
            zenrc_dds::dds_create_guardcondition(participant_entity)
        }) {
            Ok(g) => g,
            Err(e) => {
                unsafe { zenrc_dds::dds_delete(ws) };
                return Err(e);
            }
        };

        // 将守护条件附加到 WaitSet（token = WAKE_TOKEN = 0）
        if let Err(e) = check_ret(unsafe {
            zenrc_dds::dds_waitset_attach(ws, guard, WAKE_TOKEN)
        }) {
            unsafe { zenrc_dds::dds_delete(guard) };
            unsafe { zenrc_dds::dds_delete(ws) };
            return Err(e);
        }

        let core = Arc::new(ContextCore {
            waitset: ws,
            guard,
            running: AtomicBool::new(true),
            readers: Mutex::new(HashMap::new()),
        });

        let core_thread = Arc::clone(&core);
        let handle = thread::Builder::new()
            .name("dds-context".into())
            .spawn(move || context_loop(core_thread))
            .map_err(|e| DdsError::RetCode(-1, format!("创建上下文线程失败: {e}")))?;

        Ok(Self {
            participant,
            core,
            thread: Some(handle),
        })
    }

    // ── 域管理 ──────────────────────────────────────────────────────────────

    /// 获取域 ID
    pub fn domain_id(&self) -> Result<u32> {
        self.participant.domain_id()
    }

    /// 返回底层 DDS 参与者实体句柄（用于高级场景）
    pub fn entity(&self) -> dds_entity_t {
        self.participant.entity()
    }

    /// 查找同域内的所有参与者实体
    pub fn lookup_participants(domain_id: u32) -> Result<Vec<dds_entity_t>> {
        DomainParticipant::lookup_participants(domain_id)
    }

    // ── Topic 工厂 ─────────────────────────────────────────────────────────

    /// 创建带默认 QoS 的 Topic
    pub fn create_topic<T: RawMessageBridge>(&self, name: &str) -> Result<Topic<T>> {
        self.participant.create_topic(name)
    }

    /// 创建带自定义 QoS 的 Topic
    pub fn create_topic_with_qos<T: RawMessageBridge>(
        &self,
        name: &str,
        qos: &Qos,
    ) -> Result<Topic<T>> {
        self.participant.create_topic_with_qos(name, qos)
    }

    // ── Publisher 工厂 ─────────────────────────────────────────────────────

    /// 创建发布者（自动创建 Topic）
    ///
    /// # 泛型参数
    /// - `T`：安全的 Rust 消息类型，必须实现 [`RawMessageBridge`]
    pub fn create_publisher<T: RawMessageBridge>(
        &self,
        topic_name: &str,
        qos: Qos,
    ) -> Result<Publisher<T>> {
        self.participant.create_publisher(topic_name, qos)
    }

    // ── Subscription 工厂 ─────────────────────────────────────────────────

    /// 创建订阅者（自动创建 Topic），并将 ReadCondition 附加到本上下文的 WaitSet。
    ///
    /// 通过此方法创建的 `Subscription` 支持异步流（[`Subscription::into_stream`]）。
    /// 若只需同步访问，可直接使用 `ctx.participant.create_subscription()`。
    ///
    /// # 泛型参数
    /// - `T`：安全的 Rust 消息类型，必须实现 [`RawMessageBridge`]
    pub fn create_subscription<T: RawMessageBridge>(
        &self,
        topic_name: &str,
        qos: Qos,
    ) -> Result<Subscription<T>> {
        let topic = self.participant.create_topic_with_qos::<T>(topic_name, &qos)?;
        let sub = unsafe {
            zenrc_dds::dds_create_subscriber(
                self.participant.entity(),
                qos.raw as *const _,
                std::ptr::null(),
            )
        };
        let reader = check_entity(unsafe {
            zenrc_dds::dds_create_reader(
                sub,
                topic.entity,
                qos.raw as *const _,
                std::ptr::null(),
            )
        })?;
        Ok(Subscription::with_context(
            reader,
            topic,
            Arc::clone(&self.core),
        ))
    }
}

impl Drop for DdsContext {
    fn drop(&mut self) {
        self.core.running.store(false, Ordering::Release);
        // 唤醒阻塞中的 dds_waitset_wait，使后台线程尽快检测到退出标志
        unsafe { zenrc_dds::dds_set_guardcondition(self.core.guard, true) };
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

// ─── 后台轮询 ─────────────────────────────────────────────────────────────────

/// 在后台 OS 线程中持续轮询 WaitSet，有条件触发时唤醒对应订阅者的 Notify。
fn context_loop(core: Arc<ContextCore>) {
    while core.running.load(Ordering::Acquire) {
        let mut xs: Vec<dds_attach_t> = vec![0; MAX_TRIGGERS];
        let n = unsafe {
            zenrc_dds::dds_waitset_wait(
                core.waitset,
                xs.as_mut_ptr(),
                MAX_TRIGGERS,
                POLL_TIMEOUT_NS,
            )
        };

        if n < 0 {
            // WaitSet 出错（实体已被删除等），退出循环
            break;
        }

        // n == 0：超时，无条件触发，继续检测 running 标志
        xs.truncate(n as usize);

        for token in xs {
            if token == WAKE_TOKEN {
                // 重置守护条件，避免持续触发
                unsafe { zenrc_dds::dds_set_guardcondition(core.guard, false) };
                continue;
            }

            // 唤醒对应订阅者：短暂持锁取 Arc<Notify>，在锁外调用，防止死锁
            #[cfg(feature = "async")]
            {
                let notify = {
                    let readers = core.readers.lock().unwrap();
                    readers.get(&token).map(|e| Arc::clone(&e.notify))
                };
                if let Some(n) = notify {
                    // notify_one 存储一个 permit，即使当前无等待方也不丢失
                    n.notify_one();
                }
            }
        }
    }
}
