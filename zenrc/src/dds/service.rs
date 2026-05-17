use std::ffi::c_void;
use std::sync::Arc;
use std::time::Duration;

use zenrc_dds::{
    DDS_ANY_STATE, RawMessageBridge, Sample, dds_entity_t,
};

use super::common::take_one;
use super::error::{DdsError, Result, check_entity, check_ret};
use super::qos::duration_to_nanos;
use super::topic::Topic;

// ─── ServiceServer ─────────────────────────────────────────────────────────────

/// DDS 服务端，监听请求主题并处理请求、发布应答。
///
/// 请求主题：`rq/{name}Request`，应答主题：`rr/{name}Reply`。
///
/// 通过 [`super::context::DdsContext::create_service`] 创建。
/// 调用 [`ServiceServer::set_event`] 注册异步事件回调。
pub struct ServiceServer<Req: RawMessageBridge, Res: RawMessageBridge> {
    reader: dds_entity_t,
    writer: dds_entity_t,
    _req_topic: Topic<Req>,
    _res_topic: Topic<Res>,
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
            notify: None,
        }
    }

    /// 创建服务端并将 reader 注册到 [`super::context::DdsContext`] 的共享 WaitSet，
    /// 设置异步 notify 句柄。
    ///
    /// 由 [`super::context::DdsContext::create_service`] 调用。
    pub(crate) fn with_context(
        &mut self,
        context: &super::context::DdsContext,
    ) {
        let notify = Some(context.attach(self.reader));
        self.notify = notify;
    }

    fn take_one_request(reader: dds_entity_t) -> Result<Option<Sample<Req>>> {
        take_one(reader)
    }

    /// 注册事件回调：当共享 WaitSet 的 notify 被唤醒时，在 tokio 任务中执行 `handler`。
    ///
    /// 该函数会启动后台任务并返回任务句柄。任务生命周期由调用方管理。
    pub fn set_event<F>(&self, handler: F) -> Result<tokio::task::JoinHandle<()>>
    where
        F: Fn(Sample<Req>) -> Res + Send + Sync + 'static,
        Req: Send + 'static,
        Res: Send + 'static,
    {
        let notify = match &self.notify {
            Some(n) => Arc::clone(n),
            None => {
                return Err(DdsError::NullPtr(
                    "ServiceServer 未附加到 DdsContext，无法设置事件回调".into(),
                ));
            }
        };

        let reader = self.reader;
        let writer = self.writer;
        let handler = Arc::new(handler);

        Ok(tokio::spawn(async move {
            loop {
                notify.notified().await;

                let sample = match Self::take_one_request(reader) {
                    Ok(Some(sample)) => sample,
                    Ok(None) => continue,
                    Err(_) => continue,
                };
                let res = (handler)(sample);
                let raw_res = res.to_raw();
                if check_ret(unsafe {
                    zenrc_dds::dds_write(writer, &raw_res as *const _ as *const c_void)
                })
                .is_err()
                {
                    break;
                }
            }
        }))
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
/// 通过 [`super::context::DdsContext::create_client`] 创建。
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
        Ok(take_one::<Res>(self.reader)?.map(|sample| sample.into_parts().0))
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
