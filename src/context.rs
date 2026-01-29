use std::mem::MaybeUninit;
use std::os::raw::c_void;

use crate::error::TVResult;
use crate::ptr::Pointer;

/// Context handle for Tashi Vertex.
///
/// The TVContext represents the main context for Tashi Vertex and must be
/// initialized before using most other functions. This opaque handle encapsulates
/// internal state and configuration required for operations.
///
pub struct Context {
    pub(crate) handle: Pointer<TVContext>,
}

impl Context {
    /// Initialize a new context for Tashi Vertex.
    pub fn new() -> crate::Result<Self> {
        let mut handle = MaybeUninit::<Pointer<TVContext>>::uninit();

        unsafe { tv_context_new(handle.as_mut_ptr()) }.ok()?;

        let handle = unsafe { handle.assume_init() };

        Ok(Self { handle })
    }
}

pub(crate) type TVContext = c_void;

unsafe extern "C" {
    fn tv_context_new(context: *mut Pointer<TVContext>) -> TVResult;
}
