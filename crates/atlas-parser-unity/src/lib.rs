//! Unity IL2CPP parser — stub crate.
//!
//! Phase 5+ deliverable. Exists in the workspace today so that the
//! `SdkParser` trait stays honest about being "implementable by more
//! than one parser" (plan §11 acceptance: stub Unity parser must ingest
//! through the same code path as the UE parser).

pub const PARSER_NAME: &str = "il2cpp-unity";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_name_is_stable() {
        assert_eq!(PARSER_NAME, "il2cpp-unity");
    }
}
