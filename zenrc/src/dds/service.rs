use std::ffi::c_void;
use std::time::Duration;

use zenrc_dds::{DDS_ANY_STATE, RawMessageBridge, dds_entity_t, dds_sample_info_t};

use super::error::{DdsError, Result, check_entity, check_ret};
use super::qos::duration_to_nanos;
use super::topic::Topic;

// ─── ServiceServer ─────────────────────────────────────────────────────────────

/// DDS 服务端，监听请求主题并发布应答。
///
/// 请求主题：`{name}/request`，应答主题：`{name}/reply`。
///
/// 通过 [`super::context::DomainParticipant::create_service_server`] 创建。
pub struct ServiceServer<Req: RawMessageBridge, Res: RawMessageBridge> {
    reader: dds_entity_t,
    writer: dds_entity_t,
    _req_topic: Topic<Req>,
    _res_topic: Topic<Res>,
}

impl<Req: RawMessageBridge, Res: RawMessageBridge> ServiceServer<Req, Res> {
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
        }
    }

    /// 尝试读取并处理一个请求，若无请求则立即返回 `Ok(false)`
    pub fn spin_once<F>(&self, handler: F) -> Result<bool>
    where
        F: FnOnce(Req) -> Res,
    {
        let mut raw: Req::CStruct = unsafe { std::mem::zeroed() };
        let mut ptr: *mut c_void = &mut raw as *mut Req::CStruct as *mut c_void;
        let mut info: dds_sample_info_t = unsafe { std::mem::zeroed() };

        let n = unsafe {
            zenrc_dds::dds_take(self.reader, &mut ptr, &mut info, 1, 1)
        };

        if n < 0 {
            return Err(DdsError::RetCode(n, "dds_take failed".into()));
        }
        if n == 0 || !info.valid_data {
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

    /// 持续处理请求，直到 `handler` 返回 `None`
    pub fn spin<F>(&self, mut handler: F) -> Result<()>
    where
        F: FnMut(Req) -> Option<Res>,
    {
        loop {
            let mut raw: Req::CStruct = unsafe { std::mem::zeroed() };
            let mut ptr: *mut c_void = &mut raw as *mut Req::CStruct as *mut c_void;
            let mut info: dds_sample_info_t = unsafe { std::mem::zeroed() };

            let n = unsafe {
                zenrc_dds::dds_take(self.reader, &mut ptr, &mut info, 1, 1)
            };

            if n < 0 {
                return Err(DdsError::RetCode(n, "dds_take failed".into()));
            }
            if n > 0 && info.valid_data {
                let req = Req::from_raw(raw);
                match handler(req) {
                    Some(res) => {
                        let raw_res = res.to_raw();
                        check_ret(unsafe {
                            zenrc_dds::dds_write(
                                self.writer,
                                &raw_res as *const _ as *const c_void,
                            )
                        })?;
                    }
                    None => break,
                }
            } else {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        Ok(())
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
/// 通过 [`super::context::DomainParticipant::create_service_client`] 创建。
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
