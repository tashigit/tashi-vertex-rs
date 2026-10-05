use std::ffi::{CString, c_char};
use std::mem::{self, MaybeUninit};
use std::os::raw::c_void;
use std::time::Duration;

use crate::context::TVContext;
use crate::error::TVResult;
use crate::key_public::KEY_PUBLIC_DER_LENGTH;
use crate::message::Message;
use crate::options::TVOptions;
use crate::peers::TVPeers;
use crate::ptr::Pointer;
use crate::socket::TVSocket;
use crate::{Context, KeyPublic, KeySecret, Options, PeerCapabilities, Peers, Socket, Transaction};

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
        joining_running_session: bool,
    ) -> crate::Result<Self> {
        let mut socket_ptr = socket.handle.as_ptr();
        let mut options_ptr = options.handle.as_ptr();
        let mut peers_ptr = peers.handle.as_ptr();

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
                joining_running_session
            )
        }
        .ok()?;

        let handle = unsafe { handle.assume_init() };

        Ok(Self { handle })
    }

    /// Listens for the next incoming message on the given engine.
    pub async fn recv_message(&self) -> crate::Result<Option<Message>> {
        Message::recv(self).await
    }

    /// Sends a transaction to the network.
    pub fn send_transaction(&self, transaction: Transaction) -> crate::Result<()> {
        transaction.send(self)
    }

    /// Returns the public keys of the current active voting creators
    /// (the live BFT quorum membership).
    pub fn active_creators(&self) -> crate::Result<Vec<KeyPublic>> {
        let mut capacity = 16;

        loop {
            let mut buffer = vec![0u8; capacity * KEY_PUBLIC_DER_LENGTH];
            let mut count = 0;

            unsafe {
                tv_engine_get_active_creators(
                    self.handle.as_ptr(),
                    buffer.as_mut_ptr(),
                    capacity,
                    &mut count,
                )
            }
            .ok()?;

            // count is the total number of active creators; if it exceeds the
            // capacity we passed, only the first `capacity` keys were written,
            // so retry with a buffer large enough for all of them
            if count <= capacity {
                return buffer[..count * KEY_PUBLIC_DER_LENGTH]
                    .chunks_exact(KEY_PUBLIC_DER_LENGTH)
                    .map(KeyPublic::from_der)
                    .collect();
            }

            capacity = count;
        }
    }

    /// Votes to (re-)admit a creator into the active voting set.
    ///
    /// A node that was kicked is no longer an active creator; restarting it
    /// with `joining_running_session` lets it sync but does not make it a voter
    /// again. Re-admission requires a supermajority of the current voters to
    /// call this for the creator being recovered.
    ///
    /// The address, public key, and capabilities must describe the creator
    /// exactly as in [`Peers::insert`]. The vote stays active for `timeout`
    /// waiting for the supermajority to gather.
    pub fn vote_add_node(
        &self,
        address: &str,
        public: &KeyPublic,
        capabilities: PeerCapabilities,
        timeout: Duration,
    ) -> crate::Result<()> {
        let address = CString::new(address).map_err(|_| crate::Error::Argument)?;

        let res = unsafe {
            tv_engine_vote_add_node(
                self.handle.as_ptr(),
                address.as_ptr(),
                public,
                capabilities.to_flags() as u8,
                timeout.as_secs(),
            )
        };

        res.ok()
    }

    /// Gracefully stops the engine.
    ///
    /// Once the engine has wound down, in-flight and subsequent calls to
    /// [`Engine::recv_message`] return `None`, so a consumer blocked on receive
    /// observes the stop even when no further consensus messages are flowing
    /// (e.g. the session has lost quorum). Non-blocking and idempotent; safe to
    /// call from the thread driving the engine.
    pub fn stop(&self) -> crate::Result<()> {
        unsafe { tv_engine_stop(self.handle.as_ptr()) }.ok()
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
        joining_running_session: bool
    ) -> TVResult;

    fn tv_engine_get_active_creators(
        engine: *const TVEngine,
        out_buf: *mut u8,
        cap_count: usize,
        out_count: *mut usize,
    ) -> TVResult;

    fn tv_engine_vote_add_node(
        engine: *const TVEngine,
        address: *const c_char,
        public: &KeyPublic,
        capabilities: u8,
        timeout_secs: u64,
    ) -> TVResult;

    fn tv_engine_stop(engine: *const TVEngine) -> TVResult;
}
