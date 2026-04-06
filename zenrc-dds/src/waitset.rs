use std::sync::Arc;
use std::time::Duration;

use crate::domain::ParticipantInner;
use crate::error::{check_entity, check_ret, DdsError, Result};
use crate::publisher::Publisher;
use crate::qos::duration_to_nanos;
use crate::subscriber::Subscription;
use crate::topic::DdsMsg;
use crate::{dds_attach_t, dds_entity_t};

/// 等待结果：触发了等待集的实体列表（每个元素对应 attach 时传入的 token）
pub type WaitResult = Vec<dds_attach_t>;

/// DDS 等待集（WaitSet），用于同步等待多个条件（读者、守护条件等）。
///
/// # 示例
/// ```no_run
/// use zenrc_dds::waitset::WaitSet;
/// use std::time::Duration;
///
/// let mut ws = WaitSet::new(&participant).unwrap();
/// ws.attach_reader(&subscription, 1).unwrap();
///
/// loop {
///     let triggered = ws.wait(Duration::from_secs(1)).unwrap();
///     if triggered.contains(&1) {
///         // subscription 有数据可读
///     }
/// }
/// ```
pub struct WaitSet {
    entity: dds_entity_t,
    _participant: Arc<ParticipantInner>,
}

impl WaitSet {
    /// 在指定参与者下创建等待集
    pub fn new(participant: &crate::domain::DomainParticipant) -> Result<Self> {
        let entity = unsafe { crate::dds_create_waitset(participant.entity()) };
        let entity = check_entity(entity)?;
        Ok(Self {
            entity,
            _participant: Arc::clone(&participant.inner),
        })
    }

    // ── 附加条件 ──────────────────────────────────────────────────────────────

    /// 将订阅者的读者实体附加到等待集，`token` 用于在 [`WaitSet::wait`] 结果中识别
    pub fn attach_reader<T: DdsMsg>(
        &self,
        subscription: &Subscription<T>,
        token: isize,
    ) -> Result<()> {
        check_ret(unsafe {
            crate::dds_waitset_attach(self.entity, subscription.entity(), token)
        })
    }

    /// 将发布者的写者实体附加到等待集（等待发布匹配事件）
    pub fn attach_writer<T: DdsMsg>(
        &self,
        publisher: &Publisher<T>,
        token: isize,
    ) -> Result<()> {
        check_ret(unsafe {
            crate::dds_waitset_attach(self.entity, publisher.entity(), token)
        })
    }

    /// 附加任意 DDS 实体句柄
    pub fn attach_entity(&self, entity: dds_entity_t, token: isize) -> Result<()> {
        check_ret(unsafe { crate::dds_waitset_attach(self.entity, entity, token) })
    }

    /// 从等待集中移除实体
    pub fn detach_entity(&self, entity: dds_entity_t) -> Result<()> {
        check_ret(unsafe { crate::dds_waitset_detach(self.entity, entity) })
    }

    // ── 守护条件 ──────────────────────────────────────────────────────────────

    /// 创建守护条件（GuardCondition），允许外部线程触发等待集
    pub fn create_guard_condition(&self) -> Result<GuardCondition> {
        let entity = unsafe { crate::dds_create_guardcondition(self.entity) };
        let entity = check_entity(entity)?;
        Ok(GuardCondition { entity })
    }

    // ── 等待 ──────────────────────────────────────────────────────────────────

    /// 阻塞等待，直到有条件触发或超时
    ///
    /// 返回触发条件对应的 token 列表（可能为空，表示超时）
    pub fn wait(&self, timeout: Duration) -> Result<WaitResult> {
        self.wait_until_ns(duration_to_nanos(timeout))
    }

    /// 阻塞等待，使用绝对时间戳（DDS 纪元纳秒）
    pub fn wait_abs(&self, abs_timestamp_ns: i64) -> Result<WaitResult> {
        const MAX_TRIGGERS: usize = 32;
        let mut xs: Vec<dds_attach_t> = vec![0; MAX_TRIGGERS];
        let n = unsafe {
            crate::dds_waitset_wait_until(
                self.entity,
                xs.as_mut_ptr(),
                MAX_TRIGGERS,
                abs_timestamp_ns,
            )
        };
        self.handle_wait_result(n, xs)
    }

    /// 手动触发等待集（用于外部唤醒）
    pub fn trigger(&self) -> Result<()> {
        check_ret(unsafe { crate::dds_waitset_set_trigger(self.entity, true) })
    }

    /// 获取等待集中所有已附加的实体句柄
    pub fn attached_entities(&self) -> Result<Vec<dds_entity_t>> {
        const MAX: usize = 64;
        let mut buf = vec![0i32; MAX];
        let n = unsafe {
            crate::dds_waitset_get_entities(self.entity, buf.as_mut_ptr(), MAX)
        };
        let n = check_entity(n)? as usize;
        buf.truncate(n);
        Ok(buf)
    }

    /// 返回底层 DDS 实体句柄
    pub fn entity(&self) -> dds_entity_t {
        self.entity
    }

    // ── 内部 ──────────────────────────────────────────────────────────────────

    fn wait_until_ns(&self, timeout_ns: i64) -> Result<WaitResult> {
        const MAX_TRIGGERS: usize = 32;
        let mut xs: Vec<dds_attach_t> = vec![0; MAX_TRIGGERS];
        let n = unsafe {
            crate::dds_waitset_wait(
                self.entity,
                xs.as_mut_ptr(),
                MAX_TRIGGERS,
                timeout_ns,
            )
        };
        self.handle_wait_result(n, xs)
    }

    fn handle_wait_result(
        &self,
        n: crate::dds_return_t,
        mut xs: Vec<dds_attach_t>,
    ) -> Result<WaitResult> {
        if n < 0 {
            return Err(DdsError::RetCode(n, "dds_waitset_wait failed".into()));
        }
        xs.truncate(n as usize);
        Ok(xs)
    }
}

impl Drop for WaitSet {
    fn drop(&mut self) {
        unsafe { crate::dds_delete(self.entity) };
    }
}

unsafe impl Send for WaitSet {}
unsafe impl Sync for WaitSet {}

// ─── GuardCondition ────────────────────────────────────────────────────────────

/// DDS 守护条件，可由外部线程触发以唤醒等待集。
pub struct GuardCondition {
    entity: dds_entity_t,
}

impl GuardCondition {
    /// 触发守护条件（唤醒等待此条件的等待集）
    pub fn trigger(&self) -> Result<()> {
        check_ret(unsafe { crate::dds_set_guardcondition(self.entity, true) })
    }

    /// 清除触发状态
    pub fn reset(&self) -> Result<()> {
        check_ret(unsafe { crate::dds_set_guardcondition(self.entity, false) })
    }

    /// 读取当前触发状态（读取后不清除）
    pub fn is_triggered(&self) -> Result<bool> {
        let mut triggered = false;
        check_ret(unsafe { crate::dds_read_guardcondition(self.entity, &mut triggered) })?;
        Ok(triggered)
    }

    /// 读取并清除触发状态
    pub fn take_triggered(&self) -> Result<bool> {
        let mut triggered = false;
        check_ret(unsafe { crate::dds_take_guardcondition(self.entity, &mut triggered) })?;
        Ok(triggered)
    }

    /// 返回底层 DDS 实体句柄
    pub fn entity(&self) -> dds_entity_t {
        self.entity
    }
}

impl Drop for GuardCondition {
    fn drop(&mut self) {
        unsafe { crate::dds_delete(self.entity) };
    }
}

unsafe impl Send for GuardCondition {}
unsafe impl Sync for GuardCondition {}
