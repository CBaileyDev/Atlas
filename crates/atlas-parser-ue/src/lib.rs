//! Dumper-7 parser for Unreal Engine SDK dumps.
//!
//! Phase 0 ships only an empty crate that compiles against
//! `atlas-parser-trait`. The actual lexer + recursive-descent parser
//! lands in Phase 1 (plan §7).

/// Stable identifier for this parser, returned by `SdkParser::name()`
/// once the trait is implemented in Phase 1.
pub const PARSER_NAME: &str = "dumper7-ue";

/// Crate version, returned alongside `PARSER_NAME` when ingesting a dump.
pub const PARSER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_name_is_stable() {
        assert_eq!(PARSER_NAME, "dumper7-ue");
    }

    #[test]
    fn links_parser_trait_crate() {
        // Phase 0 acceptance: this crate compiles and links against
        // atlas-parser-trait.
        assert_eq!(atlas_parser_trait::crate_smoke(), "atlas-parser-trait");
    }
}
