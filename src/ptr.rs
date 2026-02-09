use std::{ffi::c_void, ptr::NonNull};

/// Opaque pointer handle for values managed by Tashi Vertex.
#[repr(C)]
pub struct Pointer<T> {
    ptr: NonNull<T>,
}

impl<T> Pointer<T> {
    pub unsafe fn from_ptr_unchecked(ptr: *mut T) -> Self {
        Self {
            ptr: unsafe { NonNull::new_unchecked(ptr) },
        }
    }

    pub const fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }
}

unsafe impl<T> Send for Pointer<T> {}
unsafe impl<T> Sync for Pointer<T> {}

/// Free memory associated with a Tashi Vertex pointer.
impl<T> Drop for Pointer<T> {
    fn drop(&mut self) {
        unsafe {
            tv_free(self.ptr.as_ptr().cast());
        }
    }
}

unsafe extern "C" {
    unsafe fn tv_free(ptr: *mut c_void);
}
