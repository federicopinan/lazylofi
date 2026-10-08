//! Cross-cutting error helpers shared across the audio pipeline.

/// Returns `true` when `err` is a [`reqwest::Error`] whose `is_timeout()`
/// reports `true`. Used by the retry loops in the downloader and the
/// `Next`-track handler to skip the backoff sleep when the underlying
/// failure was just a reqwest timeout.
pub fn is_reqwest_timeout(err: &eyre::Report) -> bool {
    err.downcast_ref::<reqwest::Error>()
        .is_some_and(reqwest::Error::is_timeout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eyre::eyre;

    #[test]
    fn non_reqwest_error_is_not_timeout() {
        let err = eyre!("some random failure");
        assert!(!is_reqwest_timeout(&err));
    }

    // Constructing a real `reqwest::Error` from outside reqwest itself
    // requires plumbing through a `Client`, and the public API keeps it
    // locked behind the builder. Skip the positive case rather than
    // fight the type system — the helper is one line and obvious.
}
