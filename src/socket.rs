use std::ffi::{CString, c_char};
use std::os::raw::c_void;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task;

use crate::Context;
use crate::context::TVContext;
use crate::error::TVResult;
use crate::ptr::Pointer;

/// Handle to a Tashi Vertex socket.
pub struct Socket {
    #[allow(unused)]
    pub(crate) handle: Pointer<TVSocket>,
}

impl Socket {
    /// Binds a Tashi Vertex (TV) socket to the specified address.
    ///
    /// Note that the address must be a valid IPv4 or IPv6 address, including the port number.
    /// A DNS lookup is not performed.
    ///
    pub fn bind(context: &Context, address: &str) -> impl Future<Output = crate::Result<Self>> {
        let address = CString::new(address).unwrap();

        SocketBind {
            context,
            address,
            invoked: false,
            shared: Arc::new(Mutex::new(SharedBindState {
                waker: None,
                result: None,
            })),
        }
    }
}

/// State shared between the Future and the C callback via Arc.
/// Ensures the callback always writes to valid memory even if the
/// Future is dropped (e.g. by tokio::select! or timeout).
struct SharedBindState {
    waker: Option<task::Waker>,
    result: Option<crate::Result<Socket>>,
}

struct SocketBind<'a> {
    context: &'a Context,
    address: CString,
    invoked: bool,
    shared: Arc<Mutex<SharedBindState>>,
}

impl Future for SocketBind<'_> {
    type Output = crate::Result<Socket>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        extern "C" fn callback(
            result: TVResult,
            socket: Pointer<TVSocket>,
            user_data: *mut c_void,
        ) {
            let shared = unsafe { Arc::from_raw(user_data as *const Mutex<SharedBindState>) };

            let bind_result = result.ok_with(socket).map(|handle| Socket { handle });

            let mut state = shared.lock().unwrap();
            state.result = Some(bind_result);
            if let Some(waker) = state.waker.take() {
                waker.wake();
            }
        }

        if !self.invoked {
            {
                let mut state = self.shared.lock().unwrap();
                state.waker = Some(cx.waker().clone());
            }
            self.invoked = true;

            let shared_ptr = Arc::into_raw(Arc::clone(&self.shared));

            let res = unsafe {
                tv_socket_bind(
                    self.context.handle.as_ptr(),
                    self.address.as_ptr(),
                    callback,
                    shared_ptr as *mut c_void,
                )
            };

            if let Err(error) = res.ok() {
                unsafe { Arc::from_raw(shared_ptr) };
                return task::Poll::Ready(Err(error));
            }
        } else {
            let mut state = self.shared.lock().unwrap();
            if let Some(result) = state.result.take() {
                return task::Poll::Ready(result);
            }
            state.waker = Some(cx.waker().clone());
        }

        task::Poll::Pending
    }
}

pub(crate) type TVSocket = c_void;

type TVSocketBindCallback =
    extern "C" fn(result: TVResult, socket: Pointer<TVSocket>, user_data: *mut c_void);

unsafe extern "C" {
    fn tv_socket_bind(
        context: *mut TVContext,
        address: *const c_char,
        callback: TVSocketBindCallback,
        user_data: *mut c_void,
    ) -> TVResult;
}
