//! Kernel-specific error types.

use omtae_types::error::OMTAEError;
use thiserror::Error;

/// Kernel error type wrapping OMTAEError with kernel-specific context.
#[derive(Error, Debug)]
pub enum KernelError {
    /// A wrapped OMTAEError.
    #[error(transparent)]
    OMTAE(#[from] OMTAEError),

    /// The kernel failed to boot.
    #[error("Boot failed: {0}")]
    BootFailed(String),
}

/// Alias for kernel results.
pub type KernelResult<T> = Result<T, KernelError>;
