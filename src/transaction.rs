use std::ffi::c_void;
use std::mem::{self, MaybeUninit};
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::slice::{from_raw_parts, from_raw_parts_mut};

use crate::Engine;
use crate::error::TVResult;

#[must_use]
pub struct Transaction {
    data: NonNull<u8>,
    size: usize,
}

impl Transaction {
    /// Allocates a buffer for a transaction of the specified size.
    pub fn allocate(size: usize) -> Self {
        let mut data = MaybeUninit::<NonNull<u8>>::uninit();

        unsafe { tv_transaction_allocate(size, data.as_mut_ptr().cast()) }.assert_ok();

        let data = unsafe { data.assume_init() };

        Self { data, size }
    }

    /// Sends the transaction to the network via the specified engine.
    pub(crate) fn send(self, engine: &Engine) -> crate::Result<()> {
        let data = self.data.as_ptr();
        let size = self.size;

        // The engine takes ownership of the buffer and reclaims it, so suppress
        // our `Drop` to avoid freeing it here (which would double-free).
        mem::forget(self);

        unsafe { tv_transaction_send(engine.handle.as_ptr(), data, size) }.ok()
    }
}

/// Frees the buffer allocated by [`Transaction::allocate`]. Because
/// [`send`](Transaction::send) `mem::forget`s the transaction once the engine
/// takes ownership, this only runs for a transaction dropped before being sent
/// — so there is no double-free.
impl Drop for Transaction {
    fn drop(&mut self) {
        unsafe { tv_transaction_free(self.data.as_ptr(), self.size) }.assert_ok();
    }
}

impl Deref for Transaction {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { from_raw_parts(self.data.as_ptr(), self.size) }
    }
}

impl DerefMut for Transaction {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { from_raw_parts_mut(self.data.as_ptr(), self.size) }
    }
}

unsafe extern "C" {
    fn tv_transaction_allocate(size: usize, data: *mut *mut c_void) -> TVResult;

    fn tv_transaction_send(engine: *mut c_void, data: *const u8, size: usize) -> TVResult;

    fn tv_transaction_free(data: *mut u8, size: usize) -> TVResult;
}

#[cfg(test)]
mod tests {
    use super::Transaction;

    /// Allocate-then-drop without sending must reclaim the buffer via
    /// `tv_transaction_free` (TAS-94). A mismatched deallocator, double-free, or
    /// wrong size would abort the process here; repeating across sizes also
    /// gives a leak check teeth under a leak-sanitized run.
    #[test]
    fn allocate_then_drop_unsent_is_sound() {
        for size in [1usize, 64, 4096, 65536] {
            for _ in 0..1000 {
                let mut tx = Transaction::allocate(size);
                // Touch every byte so the buffer must be a valid `size`-byte
                // allocation, then let `Drop` free it.
                tx.fill(0xAB);
                assert_eq!(tx.len(), size);
                drop(tx);
            }
        }
    }
}
