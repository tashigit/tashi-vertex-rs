use std::mem::{self, MaybeUninit};
use std::os::raw::c_void;

use crate::context::TVContext;
use crate::error::TVResult;
use crate::options::TVOptions;
use crate::peers::TVPeers;
use crate::ptr::Pointer;
use crate::socket::TVSocket;
use crate::{Context, KeySecret, Options, Peers, Socket};

/// Handle for the Tashi Vertex (TV) engine.
pub struct Engine {
    pub(crate) handle: Pointer<TVEngine>,
}

impl Engine {
    /// Starts the consensus engine.
    pub fn start(
        context: &Context,
        socket: Socket,
        options: Options,
        secret: &KeySecret,
        peers: Peers,
    ) -> crate::Result<Self> {
        let mut socket_ptr = unsafe { socket.handle.as_ptr() };
        let mut options_ptr = unsafe { options.handle.as_ptr() };
        let mut peers_ptr = unsafe { peers.handle.as_ptr() };

        // ownership of these pointers is transferred to the engine
        mem::forget(socket);
        mem::forget(options);
        mem::forget(peers);

        let mut handle = MaybeUninit::<Pointer<TVEngine>>::uninit();

        unsafe {
            tv_engine_start(
                context.handle.as_ptr(),
                &mut socket_ptr,
                &mut options_ptr,
                secret,
                &mut peers_ptr,
                handle.as_mut_ptr(),
            )
        }
        .ok()?;

        let handle = unsafe { handle.assume_init() };

        Ok(Self { handle })
    }
}

pub(crate) type TVEngine = c_void;

unsafe extern "C" {
    fn tv_engine_start(
        context: *mut TVContext,
        socket: *mut *mut TVSocket,
        options: *mut *mut TVOptions,
        secret: *const KeySecret,
        peers: *mut *mut TVPeers,
        engine: *mut Pointer<TVEngine>,
    ) -> TVResult;
}
