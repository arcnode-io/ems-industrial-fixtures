//! Library root for mock-dnp3-outstation.

/// Default port for this fixture.
pub const DEFAULT_PORT: u16 = 0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_default_port() {
        assert_eq!(DEFAULT_PORT, 0);
    }
}
