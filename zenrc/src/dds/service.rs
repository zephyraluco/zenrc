use std::ffi::c_void;
#[cfg(feature = "async")]
use std::sync::Arc;
use std::time::Duration;

use zenrc_dds::{DDS_ANY_STATE, RawMessageBridge, dds_entity_t, dds_sample_info_t};

use super::error::{DdsError, Result, check_entity, check_ret};
use super::qos::duration_to_nanos;
use super::topic::Topic;

// ─── ServiceServer ─────────────────────────────────────────────────────────────

/// DDS 服务端，监听请求主题并处理请求、发布应答。
///
/// 请求主题：`rq/{name}Request`，应答主题：`rr/{name}Reply`。
///
/// 通过 [`super::context::DdsContext::create_service`] 创建。
/// 调用 [`ServiceServer::next`] 驱动请求处理。
pub struct ServiceServer<Req: RawMessageBridge, Res: RawMessageBridge> {
    reader: dds_entity_t,
    writer: dds_entity_t,
    _req_topic: Topic<Req>,
    _res_topic: Topic<Res>,
    #[cfg(feature = "async")]
    notify: Option<Arc<tokio::sync::Notify>>,
}

impl<Req: RawMessageBridge, Res: RawMessageBridge> ServiceServer<Req, Res> {
    /// 创建服务端（不附加到任何 DdsContext）。
    pub(crate) fn new(
        reader: dds_entity_t,
        writer: dds_entity_t,
        req_topic: Topic<Req>,
        res_topic: Topic<Res>,
    ) -> Self {
        Self {
            reader,
            writer,
            _req_topic: req_topic,
            _res_topic: res_topic,
            #[cfg(feature = "async")]
            notify: None,
        }
    }

    /// 创建服务端并将 reader 注册到 [`super::context::DdsContext`] 的共享 WaitSet，
    /// 设置异步 notify 句柄。
    ///
    /// 由 [`super::context::DdsContext::create_service`] 调用。
    pub(crate) fn with_context(
        reader: dds_entity_t,
        writer: dds_entity_t,
        req_topic: Topic<Req>,
        res_topic: Topic<Res>,
        context: &super::context::DdsContext,
    ) -> Self {
        #[cfg(feature = "async")]
        let notify = Some(context.attach(reader));
        #[cfg(not(feature = "async"))]
        let _ = context;
        Self {
            reader,
            writer,
            _req_topic: req_topic,
            _res_topic: res_topic,
            #[cfg(feature = "async")]
            notify,
        }
    }

    /// 等待下一条请求并调用 `handler` 处理。
    ///
    /// 通过 [`super::context::DdsContext`] 的共享 WaitSet 触发的 [`tokio::sync::Notify`] 等待，
    /// 收到通知后立即调用 `dds_take` 取出请求，无需创建独立 WaitSet。
    ///
    /// - 返回 `Ok(true)`：成功处理一条请求。
    /// - 返回 `Ok(false)`：收到通知但数据无效（已被其他路径消费）。
    /// - 返回 `Err`：`notify` 未设置（未通过 `with_context` 创建）或 DDS 操作失败。
    #[cfg(feature = "async")]
    pub async fn next<F>(&self, handler: F) -> Result<bool>
    where
        F: FnOnce(Req) -> Res,
    {
        let notify = match &self.notify {
            Some(n) => n,
            None => {
                return Err(DdsError::NullPtr(
                    "ServiceServer 未附加到 DdsContext，无法使用 next".into(),
                ));
            }
        };
        notify.notified().await;
        let mut raw: Req::CStruct = unsafe { std::mem::zeroed() };
        let mut ptr: *mut c_void = &mut raw as *mut Req::CStruct as *mut c_void;
        let mut info: dds_sample_info_t = unsafe { std::mem::zeroed() };
        let taken = unsafe { zenrc_dds::dds_take(self.reader, &mut ptr, &mut info, 1, 1) };
        if taken <= 0 || !info.valid_data {
            return Ok(false);
        }
        let req = Req::from_raw(raw);
        let res = handler(req);
        let raw_res = res.to_raw();
        check_ret(unsafe {
            zenrc_dds::dds_write(self.writer, &raw_res as *const _ as *const c_void)
        })?;
        Ok(true)
    }

}

impl<Req: RawMessageBridge, Res: RawMessageBridge> Drop for ServiceServer<Req, Res> {
    fn drop(&mut self) {
        unsafe { zenrc_dds::dds_delete(self.writer) };
        unsafe { zenrc_dds::dds_delete(self.reader) };
    }
}

// SAFETY: dds_entity_t 只是 i32，DDS 内部线程安全
unsafe impl<Req: RawMessageBridge, Res: RawMessageBridge> Send for ServiceServer<Req, Res> {}
unsafe impl<Req: RawMessageBridge, Res: RawMessageBridge> Sync for ServiceServer<Req, Res> {}

// ─── ServiceClient ─────────────────────────────────────────────────────────────

/// DDS 服务客户端，发送请求并阻塞等待应答。
///
/// 通过 [`super::context::DomainParticipant::create_client`] 创建。
pub struct ServiceClient<Req: RawMessageBridge, Res: RawMessageBridge> {
    writer: dds_entity_t,
    reader: dds_entity_t,
    participant: dds_entity_t,
    _req_topic: Topic<Req>,
    _res_topic: Topic<Res>,
}

impl<Req: RawMessageBridge, Res: RawMessageBridge> ServiceClient<Req, Res> {
    pub(crate) fn new(
        writer: dds_entity_t,
        reader: dds_entity_t,
        participant: dds_entity_t,
        req_topic: Topic<Req>,
        res_topic: Topic<Res>,
    ) -> Self {
        Self {
            writer,
            reader,
            participant,
            _req_topic: req_topic,
            _res_topic: res_topic,
        }
    }

    /// 发送请求并阻塞等待应答，超时则返回 `Ok(None)`
    pub fn call(&self, req: Req, timeout: Duration) -> Result<Option<Res>> {
        // 发送请求
        let raw_req = req.to_raw();
        check_ret(unsafe {
            zenrc_dds::dds_write(self.writer, &raw_req as *const _ as *const c_void)
        })?;

        // 创建临时 WaitSet + ReadCondition，等待应答到达
        let ws = check_entity(unsafe { zenrc_dds::dds_create_waitset(self.participant) })?;
        let cond = match check_entity(unsafe {
            zenrc_dds::dds_create_readcondition(self.reader, DDS_ANY_STATE)
        }) {
            Ok(c) => c,
            Err(e) => {
                unsafe { zenrc_dds::dds_delete(ws) };
                return Err(e);
            }
        };
        if let Err(e) = check_ret(unsafe { zenrc_dds::dds_waitset_attach(ws, cond, 1) }) {
            unsafe { zenrc_dds::dds_delete(cond) };
            unsafe { zenrc_dds::dds_delete(ws) };
            return Err(e);
        }

        let timeout_ns = duration_to_nanos(timeout);
        let mut xs = [0isize; 4];
        let n = unsafe {
            zenrc_dds::dds_waitset_wait(ws, xs.as_mut_ptr(), xs.len(), timeout_ns)
        };

        // 清理临时 WaitSet 和条件
        unsafe { zenrc_dds::dds_waitset_detach(ws, cond) };
        unsafe { zenrc_dds::dds_delete(cond) };
        unsafe { zenrc_dds::dds_delete(ws) };

        if n <= 0 {
            // 超时或错误
            return Ok(None);
        }

        // 取出应答
        let mut raw_res: Res::CStruct = unsafe { std::mem::zeroed() };
        let mut ptr: *mut c_void = &mut raw_res as *mut Res::CStruct as *mut c_void;
        let mut info: dds_sample_info_t = unsafe { std::mem::zeroed() };

        let taken = unsafe {
            zenrc_dds::dds_take(self.reader, &mut ptr, &mut info, 1, 1)
        };

        if taken < 0 {
            return Err(DdsError::RetCode(taken, "dds_take failed".into()));
        }
        if taken == 0 || !info.valid_data {
            return Ok(None);
        }

        Ok(Some(Res::from_raw(raw_res)))
    }
}

impl<Req: RawMessageBridge, Res: RawMessageBridge> Drop for ServiceClient<Req, Res> {
    fn drop(&mut self) {
        unsafe { zenrc_dds::dds_delete(self.writer) };
        unsafe { zenrc_dds::dds_delete(self.reader) };
    }
}

// SAFETY: dds_entity_t 只是 i32，DDS 内部线程安全
unsafe impl<Req: RawMessageBridge, Res: RawMessageBridge> Send for ServiceClient<Req, Res> {}
unsafe impl<Req: RawMessageBridge, Res: RawMessageBridge> Sync for ServiceClient<Req, Res> {}
