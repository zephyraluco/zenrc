use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;
use tokio::sync::mpsc;

use super::error::Result;
use super::subscriber::Sample;
use zenrc_dds::RawMessageBridge;

// ─── SubscriptionStream<T> ─────────────────────────────────────────────────────

/// [`Subscription::into_stream`] 返回的异步流类型。
///
/// 实现 [`futures_core::Stream`]`<Item = Result<Sample<T>>>`，
/// 可与 `tokio_stream::StreamExt` 或 `futures::StreamExt` 配合使用。
///
/// 流被 drop 时，后台轮询任务自动取消（通过 `JoinHandle::abort()`）。
///
/// # 示例
/// ```ignore
/// use futures::StreamExt;
///
/// let mut stream = subscription.into_stream(16);
/// while let Some(Ok(sample)) = stream.next().await {
///     println!("收到数据: {:?}", *sample);
/// }
/// ```
pub struct SubscriptionStream<T: RawMessageBridge> {
    rx: mpsc::Receiver<Result<Sample<T>>>,
    /// 后台任务句柄；Drop 时 abort 以立即取消任务
    _task: tokio::task::JoinHandle<()>,
}

impl<T: RawMessageBridge> SubscriptionStream<T> {
    pub(crate) fn new(
        rx: mpsc::Receiver<Result<Sample<T>>>,
        task: tokio::task::JoinHandle<()>,
    ) -> Self {
        Self { rx, _task: task }
    }
}

impl<T: RawMessageBridge> Drop for SubscriptionStream<T> {
    fn drop(&mut self) {
        self._task.abort();
    }
}

impl<T: RawMessageBridge + Send + 'static> Stream for SubscriptionStream<T> {
    type Item = Result<Sample<T>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}
