use std::ffi::CString;
use std::time::Duration;

use crate::{
    dds_durability_kind_DDS_DURABILITY_PERSISTENT, dds_durability_kind_DDS_DURABILITY_TRANSIENT,
    dds_durability_kind_DDS_DURABILITY_TRANSIENT_LOCAL,
    dds_durability_kind_DDS_DURABILITY_VOLATILE, dds_history_kind_DDS_HISTORY_KEEP_ALL,
    dds_history_kind_DDS_HISTORY_KEEP_LAST,
    dds_liveliness_kind_DDS_LIVELINESS_AUTOMATIC,
    dds_liveliness_kind_DDS_LIVELINESS_MANUAL_BY_PARTICIPANT,
    dds_liveliness_kind_DDS_LIVELINESS_MANUAL_BY_TOPIC,
    dds_ownership_kind_DDS_OWNERSHIP_EXCLUSIVE, dds_ownership_kind_DDS_OWNERSHIP_SHARED,
    dds_reliability_kind_DDS_RELIABILITY_BEST_EFFORT,
    dds_reliability_kind_DDS_RELIABILITY_RELIABLE, dds_qos_t,
};
use crate::error::Result;

// ─── 枚举：可靠性 ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reliability {
    BestEffort,
    Reliable,
}

// ─── 枚举：持久性 ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    /// 挥发性：历史数据不保留
    Volatile,
    /// 瞬态本地：写者退出后历史数据销毁
    TransientLocal,
    /// 瞬态：历史数据由独立持久性服务保存
    Transient,
    /// 持久：历史数据持久化保存
    Persistent,
}

// ─── 枚举：历史策略 ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum History {
    /// 保留最新的 N 条样本
    KeepLast(i32),
    /// 保留全部样本（受资源限制约束）
    KeepAll,
}

// ─── 枚举：活跃性 ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveliness {
    Automatic,
    ManualByParticipant,
    ManualByTopic,
}

// ─── 枚举：所有权 ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    Shared,
    Exclusive,
}

// ─── DDS_DURATION_INFINITE ─────────────────────────────────────────────────────

/// DDS 无限时长（i64::MAX 纳秒）
pub const DURATION_INFINITE: i64 = i64::MAX;

/// 将 `std::time::Duration` 转换为 DDS 纳秒时间
#[inline]
pub(crate) fn duration_to_nanos(d: Duration) -> i64 {
    d.as_nanos().min(i64::MAX as u128) as i64
}

// ─── Qos ───────────────────────────────────────────────────────────────────────

/// QoS 策略容器，采用 Builder 模式链式构建。
///
/// 内部持有 [`dds_qos_t`] 裸指针；Drop 时自动释放。
pub struct Qos {
    pub(crate) raw: *mut dds_qos_t,
}

// SAFETY: DDS 内部对 Qos 对象的访问是线程安全的
unsafe impl Send for Qos {}
unsafe impl Sync for Qos {}

impl Qos {
    /// 创建一个空的 QoS 对象（使用 DDS 默认值）
    pub fn new() -> Self {
        let raw = unsafe { crate::dds_create_qos() };
        assert!(!raw.is_null(), "dds_create_qos 返回 null");
        Self { raw }
    }

    // ── Setter（Builder 风格）──────────────────────────────────────────────

    /// 设置可靠性策略
    pub fn reliability(self, kind: Reliability, max_blocking: Duration) -> Self {
        let raw_kind = match kind {
            Reliability::BestEffort => dds_reliability_kind_DDS_RELIABILITY_BEST_EFFORT,
            Reliability::Reliable => dds_reliability_kind_DDS_RELIABILITY_RELIABLE,
        };
        unsafe {
            crate::dds_qset_reliability(self.raw, raw_kind, duration_to_nanos(max_blocking));
        }
        self
    }

    /// 设置持久性策略
    pub fn durability(self, kind: Durability) -> Self {
        let raw_kind = match kind {
            Durability::Volatile => dds_durability_kind_DDS_DURABILITY_VOLATILE,
            Durability::TransientLocal => dds_durability_kind_DDS_DURABILITY_TRANSIENT_LOCAL,
            Durability::Transient => dds_durability_kind_DDS_DURABILITY_TRANSIENT,
            Durability::Persistent => dds_durability_kind_DDS_DURABILITY_PERSISTENT,
        };
        unsafe {
            crate::dds_qset_durability(self.raw, raw_kind);
        }
        self
    }

    /// 设置历史策略
    pub fn history(self, kind: History) -> Self {
        let (raw_kind, depth) = match kind {
            History::KeepLast(n) => (dds_history_kind_DDS_HISTORY_KEEP_LAST, n),
            History::KeepAll => (dds_history_kind_DDS_HISTORY_KEEP_ALL, -1),
        };
        unsafe {
            crate::dds_qset_history(self.raw, raw_kind, depth);
        }
        self
    }

    /// 设置截止时间（Deadline）
    pub fn deadline(self, period: Duration) -> Self {
        unsafe {
            crate::dds_qset_deadline(self.raw, duration_to_nanos(period));
        }
        self
    }

    /// 设置数据生命周期（Lifespan）
    pub fn lifespan(self, duration: Duration) -> Self {
        unsafe {
            crate::dds_qset_lifespan(self.raw, duration_to_nanos(duration));
        }
        self
    }

    /// 设置延迟预算（Latency Budget）
    pub fn latency_budget(self, duration: Duration) -> Self {
        unsafe {
            crate::dds_qset_latency_budget(self.raw, duration_to_nanos(duration));
        }
        self
    }

    /// 设置活跃性策略
    pub fn liveliness(self, kind: Liveliness, lease_duration: Duration) -> Self {
        let raw_kind = match kind {
            Liveliness::Automatic => dds_liveliness_kind_DDS_LIVELINESS_AUTOMATIC,
            Liveliness::ManualByParticipant => {
                dds_liveliness_kind_DDS_LIVELINESS_MANUAL_BY_PARTICIPANT
            }
            Liveliness::ManualByTopic => dds_liveliness_kind_DDS_LIVELINESS_MANUAL_BY_TOPIC,
        };
        unsafe {
            crate::dds_qset_liveliness(self.raw, raw_kind, duration_to_nanos(lease_duration));
        }
        self
    }

    /// 设置所有权策略
    pub fn ownership(self, kind: Ownership) -> Self {
        let raw_kind = match kind {
            Ownership::Shared => dds_ownership_kind_DDS_OWNERSHIP_SHARED,
            Ownership::Exclusive => dds_ownership_kind_DDS_OWNERSHIP_EXCLUSIVE,
        };
        unsafe {
            crate::dds_qset_ownership(self.raw, raw_kind);
        }
        self
    }

    /// 设置所有权强度（仅 Exclusive 模式下有效）
    pub fn ownership_strength(self, value: i32) -> Self {
        unsafe {
            crate::dds_qset_ownership_strength(self.raw, value);
        }
        self
    }

    /// 设置分区（多个）
    pub fn partition(self, partitions: &[&str]) -> Result<Self> {
        let c_strs: Vec<CString> = partitions
            .iter()
            .map(|s| CString::new(*s))
            .collect::<std::result::Result<_, _>>()?;
        let mut ptrs: Vec<*const std::os::raw::c_char> =
            c_strs.iter().map(|s| s.as_ptr()).collect();
        unsafe {
            crate::dds_qset_partition(self.raw, ptrs.len() as u32, ptrs.as_mut_ptr());
        }
        Ok(self)
    }

    /// 设置单个分区
    pub fn partition1(self, name: &str) -> Result<Self> {
        let c_name = CString::new(name)?;
        unsafe {
            crate::dds_qset_partition1(self.raw, c_name.as_ptr());
        }
        Ok(self)
    }

    /// 设置资源限制
    pub fn resource_limits(
        self,
        max_samples: i32,
        max_instances: i32,
        max_samples_per_instance: i32,
    ) -> Self {
        unsafe {
            crate::dds_qset_resource_limits(
                self.raw,
                max_samples,
                max_instances,
                max_samples_per_instance,
            );
        }
        self
    }

    /// 设置实体名称
    pub fn entity_name(self, name: &str) -> Result<Self> {
        let c_name = CString::new(name)?;
        unsafe {
            crate::dds_qset_entity_name(self.raw, c_name.as_ptr());
        }
        Ok(self)
    }

    // ── 预设 Profile（向 ROS2 看齐）───────────────────────────────────────────

    /// ROS2 `sensor_data`：BestEffort + KeepLast(5) + Volatile
    pub fn sensor_data() -> Self {
        Self::new()
            .reliability(Reliability::BestEffort, Duration::from_millis(100))
            .history(History::KeepLast(5))
            .durability(Durability::Volatile)
    }

    /// ROS2 `system_default`：使用 DDS 默认值（Reliable + KeepLast(10)）
    pub fn system_default() -> Self {
        Self::new()
            .reliability(Reliability::Reliable, Duration::from_millis(100))
            .history(History::KeepLast(10))
    }

    /// ROS2 `services_default`：Reliable + KeepLast(10)，适用于请求/响应服务
    pub fn services_default() -> Self {
        Self::new()
            .reliability(Reliability::Reliable, Duration::from_millis(1000))
            .history(History::KeepLast(10))
            .durability(Durability::Volatile)
    }

    /// ROS2 `parameters`：Reliable + KeepAll + Volatile
    pub fn parameters() -> Self {
        Self::new()
            .reliability(Reliability::Reliable, Duration::from_millis(1000))
            .history(History::KeepLast(1000))
            .durability(Durability::Volatile)
    }

    /// ROS2 `action_status_default`：Reliable + KeepLast(1) + TransientLocal
    pub fn action_status_default() -> Self {
        Self::new()
            .reliability(Reliability::Reliable, Duration::from_millis(1000))
            .history(History::KeepLast(1))
            .durability(Durability::TransientLocal)
    }

    /// ROS2 `clock`：BestEffort + KeepLast(1)
    pub fn clock() -> Self {
        Self::new()
            .reliability(Reliability::BestEffort, Duration::from_millis(100))
            .history(History::KeepLast(1))
    }

    /// 可靠 + 瞬态本地，适用于需要历史数据的订阅者
    pub fn transient_local() -> Self {
        Self::new()
            .reliability(Reliability::Reliable, Duration::from_millis(100))
            .history(History::KeepLast(1))
            .durability(Durability::TransientLocal)
    }
}

impl Default for Qos {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Qos {
    fn drop(&mut self) {
        unsafe { crate::dds_delete_qos(self.raw) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_to_nanos_zero() {
        assert_eq!(duration_to_nanos(Duration::ZERO), 0);
    }

    #[test]
    fn duration_to_nanos_one_second() {
        assert_eq!(duration_to_nanos(Duration::from_secs(1)), 1_000_000_000);
    }

    #[test]
    fn duration_to_nanos_100ms() {
        assert_eq!(duration_to_nanos(Duration::from_millis(100)), 100_000_000);
    }

    #[test]
    fn duration_to_nanos_1us() {
        assert_eq!(duration_to_nanos(Duration::from_micros(1)), 1_000);
    }

    #[test]
    fn duration_to_nanos_saturates_at_i64_max() {
        // u64::MAX 秒远超 i64::MAX 纳秒，应截断而非溢出
        let huge = Duration::from_secs(u64::MAX);
        assert_eq!(duration_to_nanos(huge), i64::MAX);
    }
}
