//! Signals that control when/if the editor redraws

use std::future::Future;

use parking_lot::{RwLock, RwLockReadGuard};
use tokio::sync::Notify;

use crate::runtime_local;

runtime_local! {
    /// A `Notify` instance that can be used to (asynchronously) request
    /// the editor to render a new frame.
    static REDRAW_NOTIFY: Notify = Notify::const_new();

    /// A `RwLock` that prevents the next frame from being
    /// drawn until an exclusive (write) lock can be acquired.
    /// This allows asynchronous tasks to acquire `non-exclusive`
    /// locks (read) to prevent the next frame from being drawn
    /// until a certain computation has finished.
    static RENDER_LOCK: RwLock<()> = RwLock::new(());
}

pub type RenderLockGuard = RwLockReadGuard<'static, ()>;

/// Requests that the editor is redrawn. The redraws are debounced (currently to
/// 30FPS) so this can be called many times without causing a ton of frames to
/// be rendered.
pub fn request_redraw() {
    REDRAW_NOTIFY.notify_one();
}

/// Creates a redraw callback that can run outside the current runtime.
pub fn request_redraw_callback() -> impl Fn() + Send + Sync + 'static {
    let notify = &*REDRAW_NOTIFY;
    move || notify.notify_one()
}

/// Returns a future that will yield once a redraw has been asynchronously
/// requested using [`request_redraw`].
pub fn redraw_requested() -> impl Future<Output = ()> {
    REDRAW_NOTIFY.notified()
}

/// Wait until all locks acquired with [`lock_frame`] have been released.
/// This function is called before rendering and is intended to allow the frame
/// to wait for async computations that should be included in the current frame.
pub fn start_frame() {
    drop(RENDER_LOCK.write());
    // exhaust any leftover redraw notifications
    let notify = REDRAW_NOTIFY.notified();
    tokio::pin!(notify);
    notify.enable();
}

/// Acquires the render lock which will prevent the next frame from being drawn
/// until the returned guard is dropped.
pub fn lock_frame() -> RenderLockGuard {
    RENDER_LOCK.read()
}

/// A zero sized type that requests a redraw via [request_redraw] when the type [Drop]s.
pub struct RequestRedrawOnDrop;

impl Drop for RequestRedrawOnDrop {
    fn drop(&mut self) {
        request_redraw();
    }
}

#[cfg(all(test, feature = "integration_test"))]
mod tests {
    use super::*;
    use std::task::{Context, Poll, Waker};

    #[test]
    fn redraw_callback_targets_original_runtime_from_another_thread() {
        let first = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let second = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let callback = {
            let _guard = first.enter();
            request_redraw_callback()
        };

        std::thread::spawn(callback).join().unwrap();

        let mut context = Context::from_waker(Waker::noop());
        {
            let _guard = second.enter();
            assert_eq!(
                std::pin::pin!(redraw_requested()).poll(&mut context),
                Poll::Pending
            );
        }
        {
            let _guard = first.enter();
            assert_eq!(
                std::pin::pin!(redraw_requested()).poll(&mut context),
                Poll::Ready(())
            );
        }
    }
}
