use std::ffi::{CStr, CString};
use std::str::from_utf8_unchecked;

use crate::error::TVResult;

/// Calculates the maximum length of a Base58 encoded string
/// given the length of the input byte array.
pub const fn encode_length(input_len: usize) -> usize {
    // n * log(256) / log(58) + 1
    (input_len * 138 / 100) + 1
}

/// Encodes a byte array into a Base58 string.
pub fn encode(input: &[u8], output: &mut [u8]) -> crate::Result<()> {
    let res = unsafe {
        // FIXME: output length should be an in/out parameter
        tv_base58_encode(
            input.as_ptr(),
            input.len(),
            output.as_mut_ptr(),
            output.len(),
        )
    };

    res.ok()
}

/// Encodes a byte array into a Base58 string.
pub(crate) fn with_encoded<const N: usize, R>(
    input: &[u8],
    callback: impl FnOnce(&str) -> R,
) -> crate::Result<R> {
    let mut output = [0_u8; N];

    // FIXME: update usage once this returns actual length
    encode(input, &mut output)?;

    let s = unsafe { CStr::from_bytes_with_nul_unchecked(&output) };
    let s = unsafe { from_utf8_unchecked(s.to_bytes()) };

    Ok(callback(s))
}

/// Encodes a byte array into a Base58 string.
pub fn encode_to_string(input: &[u8]) -> crate::Result<String> {
    let mut output = vec![0u8; encode_length(input.len())];

    // FIXME: update usage once this returns actual length
    encode(input, &mut output)?;

    // convert through CString to remove null termination
    // SAFE: output is guaranteed to be null-terminated and UTF-8
    let s = unsafe { CString::from_vec_with_nul_unchecked(output) };
    let s = unsafe { String::from_utf8_unchecked(s.into_bytes()) };

    Ok(s)
}

/// Calculates the maximum length of a decoded byte array
/// given the length of the Base58 encoded string.
pub const fn decode_length(input_len: usize) -> usize {
    // n * log(58) / log(256) + 1
    (input_len * 733 / 1000) + 1
}

/// Decodes a Base58 string into a byte array.
pub fn decode(input: &[u8], output: &mut [u8]) -> crate::Result<()> {
    let res = unsafe {
        // FIXME: output length should be an in/out parameter
        tv_base58_decode(
            input.as_ptr(),
            input.len(),
            output.as_mut_ptr(),
            output.len(),
        )
    };

    res.ok()
}

/// Decodes a Base58 string into a byte array.
pub(crate) fn with_decoded<const N: usize, R>(
    input: &[u8],
    callback: impl FnOnce(&[u8]) -> R,
) -> crate::Result<R> {
    let mut output = [0_u8; N];

    // FIXME: update usage once this returns actual length
    decode(input, &mut output)?;

    Ok(callback(&output[..]))
}

/// Decodes a Base58 string into a byte array.
pub fn decode_to_vec(input: &[u8]) -> crate::Result<Vec<u8>> {
    let mut output = vec![0u8; decode_length(input.len())];

    // FIXME: update usage once this returns actual length
    decode(input, &mut output)?;

    Ok(output)
}

unsafe extern "C" {
    fn tv_base58_encode(
        input: *const u8,
        input_len: usize,
        output: *mut u8,
        output_len: usize,
    ) -> TVResult;

    fn tv_base58_decode(
        input: *const u8,
        input_len: usize,
        output: *mut u8,
        output_len: usize,
    ) -> TVResult;
}
