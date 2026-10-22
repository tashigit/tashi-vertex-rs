use std::ffi::c_int;
use std::os::raw::c_void;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task;

use crate::engine::TVEngine;
use crate::error::TVResult;
use crate::ptr::Pointer;
use crate::{Engine, Event, SyncPoint};

const MESSAGE_EVENT: c_int = 1;

const MESSAGE_SYNC_POINT: c_int = 2;

pub enum Message {
    Event(Event),
    SyncPoint(SyncPoint),
}

impl Message {
    /// Listens for the next incoming message on the given engine.
    pub(crate) fn recv<'e>(
        engine: &'e Engine,
    ) -> impl Future<Output = crate::Result<Option<Self>>> + 'e {
        MessageRecieve {
            engine,
            invoked: false,
            // Shared state survives even if the Future is dropped while
            // the C callback is still pending. This prevents use-after-free
            // when tokio::select! drops this future.
            shared: Arc::new(Mutex::new(SharedState {
                waker: None,
                result: None,
            })),
        }
    }
}

/// State shared between the Future and the C callback via Arc.
/// This ensures the callback always writes to valid memory, even
/// if the Future has been dropped (e.g. by tokio::select!).
struct SharedState {
    waker: Option<task::Waker>,
    result: Option<crate::Result<Option<Message>>>,
}

struct MessageRecieve<'e> {
    engine: &'e Engine,
    invoked: bool,
    shared: Arc<Mutex<SharedState>>,
}

impl Future for MessageRecieve<'_> {
    type Output = crate::Result<Option<Message>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        extern "C" fn callback(
            result: TVResult,
            message: c_int,
            data: *const c_void,
            user_data: *mut c_void,
        ) {
            // user_data points to a leaked Arc<Mutex<SharedState>>.
            // We reconstruct it here to write the result and wake the task.
            let shared = unsafe { Arc::from_raw(user_data as *const Mutex<SharedState>) };

            let msg_result = match result.ok_with(message) {
                Ok(MESSAGE_EVENT) => {
                    let handle = unsafe { Pointer::from_ptr_unchecked(data as *mut _) };
                    let event = Event { handle };
                    Ok(Some(Message::Event(event)))
                }

                Ok(MESSAGE_SYNC_POINT) => {
                    let handle = unsafe { Pointer::from_ptr_unchecked(data as *mut _) };
                    let sync_point = SyncPoint { handle };
                    Ok(Some(Message::SyncPoint(sync_point)))
                }

                Ok(_) => Ok(None),
                Err(error) => Err(error),
            };

            let mut state = shared.lock().unwrap();
            state.result = Some(msg_result);
            if let Some(waker) = state.waker.take() {
                waker.wake();
            }
            // shared is dropped here, decrementing the Arc refcount.
            // If the Future was already dropped, this is the last reference
            // and the SharedState is freed safely.
        }

        if !self.invoked {
            // First poll: register the waker and invoke the C recv
            {
                let mut state = self.shared.lock().unwrap();
                state.waker = Some(cx.waker().clone());
            }
            self.invoked = true;

            // Leak an Arc clone so the C callback owns a reference.
            // This keeps SharedState alive even if the Future is dropped.
            let shared_ptr = Arc::into_raw(Arc::clone(&self.shared));

            let res = unsafe {
                tv_message_recv(
                    self.engine.handle.as_ptr(),
                    callback,
                    shared_ptr as *mut c_void,
                )
            };

            if let Err(error) = res.ok() {
                // Reclaim the leaked Arc since callback won't fire
                unsafe { Arc::from_raw(shared_ptr) };
                return task::Poll::Ready(Err(error));
            }
        } else {
            // Subsequent polls: check if the callback has delivered a result
            let mut state = self.shared.lock().unwrap();
            if let Some(result) = state.result.take() {
                return task::Poll::Ready(result);
            }
            // Update the waker in case tokio moved us to a different worker
            state.waker = Some(cx.waker().clone());
        }

        task::Poll::Pending
    }
}

type TVMessageRecvCallback =
    extern "C" fn(result: TVResult, message: c_int, data: *const c_void, user_data: *mut c_void);

unsafe extern "C" {
    fn tv_message_recv(
        engine: *const TVEngine,
        callback: TVMessageRecvCallback,
        user_data: *mut c_void,
    ) -> TVResult;
}
