//! Template registry.
//!
//! Templates are bundled into the binary via `include_str!`. A user
//! file under `<data>/templates/<name>` overrides the bundled
//! version. The override path is honored unless its mtime is older
//! than the bundled atlas binary (which can't happen at runtime; it
//! exists for future plumbing if we ever ship a "reset to default"
//! flow).

use serde::{Deserialize, Serialize};

use crate::error::{AtlasError, AtlasResult};
use crate::paths::templates_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateInfo {
    pub name: String,
    pub description: String,
    /// Suggested output filename (the user can change it in the UI).
    pub default_filename: String,
    /// If `true`, the template was loaded from the user's override
    /// directory rather than the bundled copy.
    pub overridden: bool,
}

const TRAINER_CS: &str = include_str!("../../templates/Trainer.cs.tera");
const OFFSETS_H: &str = include_str!("../../templates/Offsets.h.tera");
const IDA_MAPPING: &str = include_str!("../../templates/IDA-Mapping.txt.tera");
const SIGSCAN: &str = include_str!("../../templates/Sigscan.txt.tera");

const BUNDLED: &[(&str, &str, &str, &str)] = &[
    (
        "Trainer.cs",
        TRAINER_CS,
        "C# trainer scaffold (single-file console app, matches 2HighInternal style).",
        "Trainer.cs",
    ),
    (
        "Offsets.h",
        OFFSETS_H,
        "Flat C++ header of static constexpr offsets.",
        "Offsets.h",
    ),
    (
        "IDA-Mapping",
        IDA_MAPPING,
        "IDA mapping snippet (FQN + offset, one per line).",
        "atlas-ida.txt",
    ),
    (
        "Sigscan",
        SIGSCAN,
        "Pseudo-sigscan snippet — name + offset pairs (placeholder bytes left for user).",
        "atlas-sigs.txt",
    ),
];

pub fn available_templates() -> AtlasResult<Vec<TemplateInfo>> {
    let dir = templates_dir().ok();
    let mut out = Vec::with_capacity(BUNDLED.len());
    for (name, _source, desc, fname) in BUNDLED {
        let overridden = dir
            .as_ref()
            .map(|d| d.join(format!("{name}.tera")).is_file())
            .unwrap_or(false);
        out.push(TemplateInfo {
            name: (*name).to_string(),
            description: (*desc).to_string(),
            default_filename: (*fname).to_string(),
            overridden,
        });
    }
    Ok(out)
}

/// Load a template's source. Returns `(source, overridden)`.
pub fn load_template(name: &str) -> AtlasResult<(String, bool)> {
    // User override?
    if let Ok(dir) = templates_dir() {
        let p = dir.join(format!("{name}.tera"));
        if p.is_file() {
            let src = std::fs::read_to_string(&p)?;
            return Ok((src, true));
        }
    }
    for (n, s, _, _) in BUNDLED {
        if *n == name {
            return Ok(((*s).to_string(), false));
        }
    }
    Err(AtlasError::Export(format!("unknown template: {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_templates_load() {
        for (name, _, _, _) in BUNDLED {
            let (src, overridden) = load_template(name).unwrap();
            assert!(!src.is_empty(), "{name} should have bundled source");
            // Override status depends on env; do not assert.
            let _ = overridden;
        }
    }

    #[test]
    fn unknown_template_errors() {
        let e = load_template("DoesNotExist").unwrap_err();
        assert!(matches!(e, AtlasError::Export(_)));
    }
}
