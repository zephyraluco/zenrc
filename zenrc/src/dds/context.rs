use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use zenrc_dds::{dds_attach_t, dds_entity_t, DDS_ANY_STATE};

use super::domain::{DomainParticipant, ParticipantInner};
use super::error::{check_entity, check_ret, DdsError, Result};

// ─── 内部常量 ──────────────────────────────────────────────────────────────────

/// 守护条件 attach token，用于唤醒/停止后台线程
const WAKE_TOKEN: dds_attach_t = 0;

/// 每次 dds_waitset_wait 的超时（100 ms），兜底检测 running 标志
const POLL_TIMEOUT_NS: i64 = 100_000_000;

/// WaitSet 单次最大触发条件数
const MAX_TRIGGERS: usize = 64;

// ─── 全局单例 ──────────────────────────────────────────────────────────────────

static GLOBAL: OnceLock<Arc<DdsContextCore>> = OnceLock::new();

// ─── 内部结构 ──────────────────────────────────────────────────────────────────

struct ReaderEntry {
    /// 已附加到 WaitSet 的 ReadCondition 实体句柄
    readcond: dds_entity_t,
    /// 数据到达时唤醒等待方的通知句柄
    #[cfg(feature = "async")]
    notify: Arc<tokio::sync::Notify>,
}

struct DdsContextCore {
    waitset: dds_entity_t,
    guard: dds_entity_t,
    running: AtomicBool,
    _participant: Arc<ParticipantInner>,
    /// token（reader entity as isize）→ ReaderEntry
    readers: Mutex<HashMap<isize, ReaderEntry>>,
}

// SAFETY: dds_entity_t 是 i32 句柄，CycloneDDS 所有操作均线程安全
unsafe impl Send for DdsContextCore {}
unsafe impl Sync for DdsContextCore {}

impl Drop for DdsContextCore {
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

// ─── DdsContext ────────────────────────────────────────────────────────────────

/// DDS 全局上下文句柄（RAII）。
///
/// 调用 [`DdsContext::init`] 启动后台 WaitSet 轮询线程。之后通过
/// [`DomainParticipant`] 创建的每个 [`Subscription`](super::subscriber::Subscription)
/// 会在构造时自动将其 ReadCondition 附加到唯一的共享 WaitSet。
/// 数据到达时，后台线程通过 [`tokio::sync::Notify`] 唤醒等待中的异步 API。
///
/// `DdsContext` 被 drop 时，后台线程优雅退出（设置 `running = false` → 触发守护
/// 条件 → `join`）。
///
/// # 示例
/// ```ignore
/// let dp = DomainParticipant::new(DOMAIN_DEFAULT)?;
/// let _ctx = DdsContext::init(&dp)?;       // 程序生命周期内保持存活
///
/// let sub = dp.create_subscription::<MyMsg>("topic", Qos::default())?;
/// // sub 创建时已自动附加到 WaitSet，async_wait_for_data / into_stream 即可使用
/// ```
pub struct DdsContext {
    /// 持有一份 Arc，确保后台线程退出前 DdsContextCore 不被释放
    _core: Arc<DdsContextCore>,
    thread: Option<thread::JoinHandle<()>>,
}

// SAFETY: _core 为 Arc<DdsContextCore>（Send+Sync），thread 只在 drop（&mut self）中访问
unsafe impl Send for DdsContext {}
unsafe impl Sync for DdsContext {}

impl DdsContext {
    /// 初始化全局 DDS 上下文，启动后台 WaitSet 轮询线程。
    ///
    /// 应在程序启动时调用一次，并将返回的 `DdsContext` 保持存活至程序退出。
    /// 若上下文已初始化则幂等返回（不重复创建线程）。
    pub fn init(participant: &DomainParticipant) -> Result<Self> {
        // 幂等：已初始化则返回不带线程句柄的占位符句柄
        if let Some(core) = GLOBAL.get() {
            return Ok(Self {
                _core: Arc::clone(core),
                thread: None,
            });
        }

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

        let core = Arc::new(DdsContextCore {
            waitset: ws,
            guard,
            running: AtomicBool::new(true),
            _participant: Arc::clone(&participant.inner),
            readers: Mutex::new(HashMap::new()),
        });

        // 存入全局单例；若并发 init 竞争，以先写入的为准
        let _ = GLOBAL.set(Arc::clone(&core));
        let core = Arc::clone(GLOBAL.get().unwrap());

        let core_thread = Arc::clone(&core);
        let handle = thread::Builder::new()
            .name("dds-context".into())
            .spawn(move || context_loop(core_thread))
            .map_err(|e| DdsError::RetCode(-1, format!("创建上下文线程失败: {e}")))?;

        Ok(Self {
            _core: core,
            thread: Some(handle),
        })
    }
}

impl Drop for DdsContext {
    fn drop(&mut self) {
        if let Some(core) = GLOBAL.get() {
            core.running.store(false, Ordering::Release);
            // 唤醒阻塞中的 dds_waitset_wait，使后台线程尽快检测到退出标志
            unsafe { zenrc_dds::dds_set_guardcondition(core.guard, true) };
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

// ─── crate 内部接口（供 Subscription 调用）────────────────────────────────────

/// 将 reader 的 ReadCondition 附加到全局 WaitSet，返回数据到达通知句柄。
///
/// 若上下文未初始化则返回 `None`；调用方应回退到独立的 spawn_blocking 模式。
/// 由 [`Subscription::new`](super::subscriber::Subscription) 自动调用。
#[cfg(feature = "async")]
pub(crate) fn attach(reader: dds_entity_t) -> Option<Arc<tokio::sync::Notify>> {
    let core = GLOBAL.get()?;

    let readcond = check_entity(unsafe {
        zenrc_dds::dds_create_readcondition(reader, DDS_ANY_STATE)
    })
    .ok()?;

    let notify = Arc::new(tokio::sync::Notify::new());
    let token = reader as isize;

    {
        let mut readers = core.readers.lock().unwrap();
        readers.insert(
            token,
            ReaderEntry {
                readcond,
                notify: Arc::clone(&notify),
            },
        );
    }

    if check_ret(unsafe {
        zenrc_dds::dds_waitset_attach(core.waitset, readcond, token)
    })
    .is_err()
    {
        core.readers.lock().unwrap().remove(&token);
        unsafe { zenrc_dds::dds_delete(readcond) };
        return None;
    }

    // 唤醒后台线程以感知新附加的 ReadCondition
    unsafe { zenrc_dds::dds_set_guardcondition(core.guard, true) };

    Some(notify)
}

/// 从全局 WaitSet 移除 reader 的 ReadCondition。
/// 由 [`Subscription::drop`](super::subscriber::Subscription) 自动调用。
#[cfg(feature = "async")]
pub(crate) fn detach(reader: dds_entity_t) {
    let Some(core) = GLOBAL.get() else {
        return;
    };
    let token = reader as isize;
    let entry = core.readers.lock().unwrap().remove(&token);
    if let Some(entry) = entry {
        unsafe { zenrc_dds::dds_waitset_detach(core.waitset, entry.readcond) };
        unsafe { zenrc_dds::dds_delete(entry.readcond) };
    }
}

// ─── 后台轮询 ─────────────────────────────────────────────────────────────────

/// 在后台 OS 线程中持续轮询 WaitSet，有条件触发时唤醒对应订阅者的 Notify。
fn context_loop(core: Arc<DdsContextCore>) {
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
