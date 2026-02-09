use std::{
    fmt::{self, Debug, Display},
    mem::MaybeUninit,
    str::FromStr,
};

use serde::{Deserialize, Serialize};

use crate::{base58, error::TVResult};

const KEY_PUBLIC_DER_LENGTH: usize = 91;
const KEY_PUBLIC_DER_BASE58_LENGTH: usize = base58::encode_length(KEY_PUBLIC_DER_LENGTH);

/// A public key used for verifying signatures in Tashi Vertex.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct KeyPublic {
    material: [u8; 72],
}

impl KeyPublic {
    /// Parses a public key from DER format.
    pub fn from_der(der: &[u8]) -> crate::Result<Self> {
        let mut key = MaybeUninit::<Self>::uninit();

        // Parse the DER-encoded key
        unsafe { tv_key_public_from_der(der.as_ptr(), der.len(), key.as_mut_ptr()) }.ok()?;

        // SAFE: tv_key_public_from_der returns ok only if the structure is initialized
        Ok(unsafe { key.assume_init() })
    }

    /// Formats the public key to DER format.
    pub fn to_der(&self, output: &mut [u8]) -> crate::Result<()> {
        let res = unsafe {
            // FIXME: output length should be an in/out parameter
            tv_key_public_to_der(self, output.as_mut_ptr(), output.len())
        };

        res.ok()
    }

    /// Formats the public key to DER format.
    pub fn to_der_vec(&self) -> crate::Result<Vec<u8>> {
        let mut output = vec![0u8; KEY_PUBLIC_DER_LENGTH];

        self.to_der(&mut output)?;

        Ok(output)
    }

    /// Formats the public key to DER format.
    fn with_der<R>(&self, callback: impl FnOnce(&[u8]) -> R) -> crate::Result<R> {
        let mut output = vec![0u8; KEY_PUBLIC_DER_LENGTH];

        self.to_der(&mut output)?;

        Ok(callback(&output))
    }
}

impl Debug for KeyPublic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"{self}\"")
    }
}

impl Display for KeyPublic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.with_der(|der| {
            base58::with_encoded::<KEY_PUBLIC_DER_BASE58_LENGTH, _>(der, |s| f.pad(s)).unwrap()
        })
        .unwrap()
    }
}

impl FromStr for KeyPublic {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        base58::with_decoded::<KEY_PUBLIC_DER_LENGTH, _>(s.as_bytes(), KeyPublic::from_der)?
    }
}

impl Serialize for KeyPublic {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for KeyPublic {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

unsafe extern "C" {
    fn tv_key_public_to_der(public: *const KeyPublic, der: *mut u8, der_len: usize) -> TVResult;

    fn tv_key_public_from_der(der: *const u8, der_len: usize, public: *mut KeyPublic) -> TVResult;
}
