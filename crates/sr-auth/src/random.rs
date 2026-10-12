//! OS-backed randomness helpers.

use crate::error::AuthError;

/// Fill a fixed-size array with cryptographically secure random bytes.
pub fn bytes<const N: usize>() -> Result<[u8; N], AuthError> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).map_err(|_| AuthError::Random)?;
    Ok(buf)
}
