use std::fmt::{self, Debug, Display};
use std::mem::MaybeUninit;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::TVResult;
use crate::{KeyPublic, base58};

const KEY_SECRET_DER_LENGTH: usize = 51;
const KEY_SECRET_DER_BASE58_LENGTH: usize = base58::encode_length(KEY_SECRET_DER_LENGTH);

/// A secret key used for signing transactions in Tashi Vertex.
#[derive(Clone)]
#[repr(C)]
pub struct KeySecret {
    material: [u8; 32],
}

impl KeySecret {
    /// Returns the raw bytes of the secret key.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.material
    }

    /// Generates a new secret key.
    ///
    /// This function populates the provided TVKeySecret structure with a newly
    /// generated secret key suitable for signing transactions.
    ///
    pub fn generate() -> Self {
        let mut key = MaybeUninit::<Self>::uninit();

        unsafe {
            tv_key_secret_generate(key.as_mut_ptr());

            // SAFE: tv_key_secret_generate cannot return without initializing the structure
            key.assume_init()
        }
    }

    /// Derives the corresponding public key from this secret key.
    pub fn public(&self) -> KeyPublic {
        let mut public = MaybeUninit::<KeyPublic>::uninit();

        let res = unsafe { tv_key_secret_to_public(self, public.as_mut_ptr()) };

        // PANIC: can only return not ok if the pointer to secret is null
        res.assert_ok();

        // SAFE: tv_key_secret_to_public returns ok only if the structure is initialized
        unsafe { public.assume_init() }
    }

    /// Parses a secret key from DER format.
    pub fn from_der(der: &[u8]) -> crate::Result<Self> {
        let mut key = MaybeUninit::<Self>::uninit();

        // Parse the DER-encoded key
        unsafe { tv_key_secret_from_der(der.as_ptr(), der.len(), key.as_mut_ptr()) }.ok()?;

        // SAFE: tv_key_secret_from_der returns ok only if the structure is initialized
        Ok(unsafe { key.assume_init() })
    }

    /// Formats the secret key to DER format.
    pub fn to_der(&self, output: &mut [u8]) -> crate::Result<()> {
        let res = unsafe {
            // FIXME: output length should be an in/out parameter
            tv_key_secret_to_der(self, output.as_mut_ptr(), output.len())
        };

        res.ok()
    }

    /// Formats the secret key to DER format.
    pub fn to_der_vec(&self) -> crate::Result<Vec<u8>> {
        let mut output = vec![0u8; KEY_SECRET_DER_LENGTH];

        self.to_der(&mut output)?;

        Ok(output)
    }

    /// Formats the secret key to DER format.
    fn with_der<R>(&self, callback: impl FnOnce(&[u8]) -> R) -> crate::Result<R> {
        let mut output = vec![0u8; KEY_SECRET_DER_LENGTH];

        self.to_der(&mut output)?;

        Ok(callback(&output))
    }
}

impl Debug for KeySecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"{self}\"")
    }
}

impl Display for KeySecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.with_der(|der| {
            base58::with_encoded::<KEY_SECRET_DER_BASE58_LENGTH, _>(der, |s| f.pad(s)).unwrap()
        })
        .unwrap()
    }
}

impl FromStr for KeySecret {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        base58::with_decoded::<KEY_SECRET_DER_LENGTH, _>(s.as_bytes(), KeySecret::from_der)?
    }
}

impl Serialize for KeySecret {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for KeySecret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

unsafe extern "C" {
    fn tv_key_secret_generate(secret: *mut KeySecret);

    fn tv_key_secret_to_public(secret: *const KeySecret, public: *mut KeyPublic) -> TVResult;

    fn tv_key_secret_to_der(secret: *const KeySecret, der: *mut u8, der_len: usize) -> TVResult;

    fn tv_key_secret_from_der(der: *const u8, der_len: usize, secret: *mut KeySecret) -> TVResult;
}
