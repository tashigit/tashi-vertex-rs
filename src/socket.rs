use std::ffi::{CString, c_char};
use std::os::raw::c_void;
use std::pin::Pin;
use std::task;

use crate::{Context, Error};
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
    pub fn bind<'a>(context: &'a Context, address: &str) -> crate::Result<impl Future<Output = crate::Result<Self>> + 'a> {
        let address = CString::new(address).map_err(|_| Error::Argument)?;

        Ok(SocketBind {
            context,
            address,
            invoked: false,
            waker: None,
            result: None,
        })
    }
}

struct SocketBind<'a> {
    context: &'a Context,
    address: CString,
    invoked: bool,
    waker: Option<task::Waker>,
    result: Option<crate::Result<Socket>>,
}

impl Future for SocketBind<'_> {
    type Output = crate::Result<Socket>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        extern "C" fn callback(
            result: TVResult,
            socket: Pointer<TVSocket>,
            user_data: *mut c_void,
        ) {
            let user_data = unsafe { &mut *(user_data as *mut SocketBind) };

            user_data.result = Some(result.ok_with(socket).map(|handle| Socket { handle }));

            if let Some(waker) = user_data.waker.take() {
                waker.wake();
            }
        }

        if !self.invoked {
            // this is the first time poll is called
            // invoke the bind operation

            self.waker = Some(cx.waker().clone());
            self.invoked = true;

            let res = unsafe {
                tv_socket_bind(
                    self.context.handle.as_ptr(),
                    self.address.as_ptr(),
                    callback,
                    self.get_mut() as *mut _ as *mut c_void,
                )
            };

            if let Err(error) = res.ok() {
                return task::Poll::Ready(Err(error));
            }
        } else if let Some(result) = self.result.take() {
            return task::Poll::Ready(result);
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
