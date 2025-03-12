//! Shared utility functions

use nvim_oxi::api::Error as ApiError;

/// Convert any error into a Neovim API error
pub fn as_api_error(e: impl std::error::Error) -> ApiError {
    ApiError::Other(e.to_string())
}
