use std::ffi::{CString, c_char, c_int};
use std::mem::MaybeUninit;
use std::os::raw::c_void;

use crate::KeyPublic;
use crate::error::TVResult;
use crate::ptr::Pointer;

const PEER_NO_ORDER: c_int = 1 << 1;

const PEER_NO_LOGIC: c_int = 1 << 2;

const PEER_PUBLIC: c_int = 1 << 3;

const PEER_UNKICKABLE: c_int = 1 << 4;

#[derive(Default)]
pub struct PeerCapabilities {
    /// Peer does not contribute to determining the finalized order of events.
    pub no_order: bool,

    /// Peer does not know application logic.
    pub no_logic: bool,

    /// Peer is marked as having a stable public address (not behind NAT).
    pub public: bool,

    /// Peer cannot be kicked from the session.
    pub unkickable: bool,
}

impl PeerCapabilities {
    const fn to_flags(&self) -> c_int {
        let mut flags = 0;

        if self.no_order {
            flags |= PEER_NO_ORDER;
        }

        if self.no_logic {
            flags |= PEER_NO_LOGIC;
        }

        if self.public {
            flags |= PEER_PUBLIC;
        }

        if self.unkickable {
            flags |= PEER_UNKICKABLE;
        }

        flags
    }
}

/// Represents a unique set of peers in the network,
/// each identified by their network address and public key.
pub struct Peers {
    handle: Pointer<TVPeers>,
}

impl Peers {
    /// Creates a new empty set of peers.
    pub fn new() -> crate::Result<Self> {
        Self::with_capacity(0)
    }

    /// Creates a new empty set of peers with the specified initial capacity.
    pub fn with_capacity(capacity: usize) -> crate::Result<Self> {
        let mut handle = MaybeUninit::<Pointer<TVPeers>>::uninit();

        unsafe { tv_peers_new(capacity, handle.as_mut_ptr()) }.ok()?;

        let handle = unsafe { handle.assume_init() };

        Ok(Self { handle })
    }

    /// Inserts a new peer into the set.
    ///
    /// Note that the address must be a valid IPv4 or IPv6 address, including the port number.
    /// A DNS lookup is not performed.
    ///
    pub fn insert(
        &mut self,
        address: &str,
        public: &KeyPublic,
        capabilities: PeerCapabilities,
    ) -> crate::Result<()> {
        let address = CString::new(address).map_err(|_| crate::Error::Argument)?;
        let capabilities = capabilities.to_flags();

        let res = unsafe {
            tv_peers_insert(self.handle.as_ptr(), address.as_ptr(), public, capabilities)
        };

        res.ok()
    }
}

type TVPeers = c_void;

unsafe extern "C" {
    fn tv_peers_new(capacity: usize, peers: *mut Pointer<TVPeers>) -> TVResult;

    fn tv_peers_insert(
        peers: *mut TVPeers,
        address: *const c_char,
        public: &KeyPublic,
        capabilities: c_int,
    ) -> TVResult;
}
