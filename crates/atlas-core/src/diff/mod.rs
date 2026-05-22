//! Structural diff between two `SymbolGraph`s of the same game.
//!
//! The engine is a **pure function** (plan §3 invariant 3): it takes
//! two graphs, a config, and user overrides; it returns a `Diff`. It
//! does not read from SQLite, does not write to disk, does not log.
//! Snapshot-testable, replayable, cheap.
//!
//! Three passes (plan §9):
//! 1. Exact match by `(kind, fqn)`.
//! 2. Fingerprint-based rename detection (Jaccard on member names +
//!    types, optionally scaled by same-module).
//! 3. Field-level classification on matched pairs.

use std::collections::{HashMap, HashSet};

use atlas_parser_trait::{RelationKind, Symbol, SymbolGraph, SymbolKind, TypeRef};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Configuration / overrides
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffConfig {
    /// Score >= this becomes a "rename suggestion" (UI shows it with `?`).
    pub fingerprint_suggestion_threshold: f64,
    /// Score >= this is treated as a confident automatic rename match.
    pub fingerprint_confidence_threshold: f64,
    pub member_name_weight: f64,
    pub member_type_weight: f64,
    pub same_module_bonus: f64,
    /// Field-name patterns that should be ignored in fingerprinting
    /// (e.g. Dumper-7 `Pad_` and synthetic `UnknownData_`).
    pub fingerprint_ignore_prefixes: Vec<String>,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            fingerprint_suggestion_threshold: 0.70,
            fingerprint_confidence_threshold: 0.90,
            member_name_weight: 0.6,
            member_type_weight: 0.3,
            same_module_bonus: 0.1,
            fingerprint_ignore_prefixes: vec!["Pad_".into(), "UnknownData_".into()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Decision {
    Match,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameOverride {
    pub base_fqn: String,
    pub head_fqn: String,
    pub decision: Decision,
}

// ---------------------------------------------------------------------------
// Diff output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SymbolRef {
    pub fqn: String,
    pub kind: SymbolKind,
    pub module: String,
}

impl SymbolRef {
    fn from_symbol(s: &Symbol) -> Self {
        Self {
            fqn: s.fqn.clone(),
            kind: s.kind,
            module: s.module.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMethod {
    Exact,
    Fingerprint,
    UserOverride,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolMatch {
    pub base: SymbolRef,
    pub head: SymbolRef,
    pub method: MatchMethod,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameSuggestion {
    pub base: SymbolRef,
    pub head: SymbolRef,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangeKind {
    OffsetChanged {
        field: String,
        old_offset: u32,
        new_offset: u32,
    },
    SizeChanged {
        field: String,
        old_size: u32,
        new_size: u32,
    },
    VtableShift {
        fn_name: String,
        old_slot: u32,
        new_slot: u32,
    },
    ParentClassChanged {
        old_parent: Option<String>,
        new_parent: Option<String>,
    },
    FieldAdded {
        field: String,
    },
    FieldRemoved {
        field: String,
    },
    FunctionSignatureChanged {
        fn_name: String,
        old_return: Option<String>,
        new_return: Option<String>,
    },
    FieldTypeSubstituted {
        field: String,
        old_type: String,
        new_type: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChange {
    pub parent_fqn: String,
    pub change: ChangeKind,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Diff {
    pub game_id: String,
    pub base_version: String,
    pub head_version: String,
    pub matches: Vec<SymbolMatch>,
    pub added: Vec<SymbolRef>,
    pub removed: Vec<SymbolRef>,
    pub renamed_suggestions: Vec<RenameSuggestion>,
    pub field_changes: Vec<FieldChange>,
}

impl Diff {
    pub fn change_summary(&self) -> ChangeSummary {
        ChangeSummary {
            matches: self.matches.len(),
            added: self.added.len(),
            removed: self.removed.len(),
            renamed_suggestions: self.renamed_suggestions.len(),
            field_changes: self.field_changes.len(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSummary {
    pub matches: usize,
    pub added: usize,
    pub removed: usize,
    pub renamed_suggestions: usize,
    pub field_changes: usize,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Compute the diff between two graphs of the same game.
///
/// `overrides` are applied first: a `Decision::Match` short-circuits
/// pass 2's scoring for that pair; a `Decision::Reject` excludes the
/// pair from suggestions.
pub fn diff(
    base: &SymbolGraph,
    head: &SymbolGraph,
    config: &DiffConfig,
    overrides: &[RenameOverride],
) -> Diff {
    let base_index = GraphIndex::build(base);
    let head_index = GraphIndex::build(head);

    let mut matched_base: HashSet<u32> = HashSet::new();
    let mut matched_head: HashSet<u32> = HashSet::new();
    let mut matches: Vec<SymbolMatch> = Vec::new();
    let mut renamed_suggestions: Vec<RenameSuggestion> = Vec::new();

    // Apply user overrides first.
    let override_index: HashMap<(&str, &str), &RenameOverride> = overrides
        .iter()
        .map(|o| ((o.base_fqn.as_str(), o.head_fqn.as_str()), o))
        .collect();

    for ov in overrides {
        match ov.decision {
            Decision::Match => {
                if let (Some(&b), Some(&h)) = (
                    base_index.by_fqn.get(ov.base_fqn.as_str()),
                    head_index.by_fqn.get(ov.head_fqn.as_str()),
                ) {
                    matched_base.insert(b);
                    matched_head.insert(h);
                    matches.push(SymbolMatch {
                        base: SymbolRef::from_symbol(&base.symbols[base_index.idx_of[&b]]),
                        head: SymbolRef::from_symbol(&head.symbols[head_index.idx_of[&h]]),
                        method: MatchMethod::UserOverride,
                        score: 1.0,
                    });
                }
            }
            Decision::Reject => {
                // Suggestions filter against this later.
            }
        }
    }

    // ---- Pass 1: Exact match by (kind, fqn). -------------------------------
    for sym in &base.symbols {
        if matched_base.contains(&sym.local_id) {
            continue;
        }
        if !is_top_level(sym.kind) {
            continue;
        }
        if let Some(&head_id) = head_index.by_fqn.get(sym.fqn.as_str()) {
            let head_sym = &head.symbols[head_index.idx_of[&head_id]];
            if head_sym.kind == sym.kind && !matched_head.contains(&head_id) {
                matched_base.insert(sym.local_id);
                matched_head.insert(head_id);
                matches.push(SymbolMatch {
                    base: SymbolRef::from_symbol(sym),
                    head: SymbolRef::from_symbol(head_sym),
                    method: MatchMethod::Exact,
                    score: 1.0,
                });
            }
        }
    }

    // ---- Pass 2: Fingerprint-based rename detection. -----------------------
    // Compute fingerprints only for classes / structs (where Dumper-7
    // emits enough internal structure to be informative).
    let base_fingerprints = compute_fingerprints(base, &base_index, config);
    let head_fingerprints = compute_fingerprints(head, &head_index, config);

    for (base_local_id, base_fp) in &base_fingerprints {
        if matched_base.contains(base_local_id) {
            continue;
        }
        let base_sym = &base.symbols[base_index.idx_of[base_local_id]];
        let mut best: Option<(u32, f64)> = None;
        for (head_local_id, head_fp) in &head_fingerprints {
            if matched_head.contains(head_local_id) {
                continue;
            }
            let head_sym = &head.symbols[head_index.idx_of[head_local_id]];
            if head_sym.kind != base_sym.kind {
                continue;
            }
            // Honor explicit Reject overrides.
            if override_index
                .get(&(base_sym.fqn.as_str(), head_sym.fqn.as_str()))
                .map(|o| matches!(o.decision, Decision::Reject))
                .unwrap_or(false)
            {
                continue;
            }
            let score = jaccard_score(base_fp, head_fp, config);
            if best.is_none() || score > best.unwrap().1 {
                best = Some((*head_local_id, score));
            }
        }

        if let Some((head_local_id, score)) = best {
            let head_sym = &head.symbols[head_index.idx_of[&head_local_id]];
            if score >= config.fingerprint_confidence_threshold {
                matched_base.insert(*base_local_id);
                matched_head.insert(head_local_id);
                matches.push(SymbolMatch {
                    base: SymbolRef::from_symbol(base_sym),
                    head: SymbolRef::from_symbol(head_sym),
                    method: MatchMethod::Fingerprint,
                    score,
                });
            } else if score >= config.fingerprint_suggestion_threshold {
                renamed_suggestions.push(RenameSuggestion {
                    base: SymbolRef::from_symbol(base_sym),
                    head: SymbolRef::from_symbol(head_sym),
                    score,
                });
            }
        }
    }

    // ---- Removed / Added (whatever wasn't matched). ------------------------
    let mut added: Vec<SymbolRef> = Vec::new();
    let mut removed: Vec<SymbolRef> = Vec::new();
    for sym in &base.symbols {
        if !is_top_level(sym.kind) {
            continue;
        }
        if !matched_base.contains(&sym.local_id) {
            // Hide things that have a pending suggestion (so the UI
            // can show them in the suggestions section, not Removed).
            if renamed_suggestions.iter().any(|r| r.base.fqn == sym.fqn) {
                continue;
            }
            removed.push(SymbolRef::from_symbol(sym));
        }
    }
    for sym in &head.symbols {
        if !is_top_level(sym.kind) {
            continue;
        }
        if !matched_head.contains(&sym.local_id) {
            if renamed_suggestions.iter().any(|r| r.head.fqn == sym.fqn) {
                continue;
            }
            added.push(SymbolRef::from_symbol(sym));
        }
    }

    // ---- Pass 3: Field-level classification on matched pairs. --------------
    let mut field_changes: Vec<FieldChange> = Vec::new();
    for m in &matches {
        if !matches!(
            m.base.kind,
            SymbolKind::Class | SymbolKind::Struct | SymbolKind::Enum
        ) {
            continue;
        }
        let base_id = base_index.by_fqn[m.base.fqn.as_str()];
        let head_id = head_index.by_fqn[m.head.fqn.as_str()];
        let changes = classify_field_changes(
            &m.head.fqn,
            base,
            base_id,
            &base_index,
            head,
            head_id,
            &head_index,
        );
        field_changes.extend(changes);
    }

    Diff {
        game_id: base.source.game_id.clone(),
        base_version: base.source.game_version.clone(),
        head_version: head.source.game_version.clone(),
        matches,
        added,
        removed,
        renamed_suggestions,
        field_changes,
    }
}

const fn is_top_level(k: SymbolKind) -> bool {
    matches!(k, SymbolKind::Class | SymbolKind::Struct | SymbolKind::Enum)
}

// ---------------------------------------------------------------------------
// Indexing helpers
// ---------------------------------------------------------------------------

struct GraphIndex {
    by_fqn: HashMap<String, u32>,
    /// Map from local_id -> position in `graph.symbols`.
    idx_of: HashMap<u32, usize>,
    /// For each parent (local_id), the list of `Contains` child local_ids.
    contains: HashMap<u32, Vec<u32>>,
    /// For each child (local_id), its parent class via `Inherits` (if any).
    inherits: HashMap<u32, u32>,
}

impl GraphIndex {
    fn build(g: &SymbolGraph) -> Self {
        let mut by_fqn = HashMap::with_capacity(g.symbols.len());
        let mut idx_of = HashMap::with_capacity(g.symbols.len());
        for (i, s) in g.symbols.iter().enumerate() {
            by_fqn.insert(s.fqn.clone(), s.local_id);
            idx_of.insert(s.local_id, i);
        }

        let mut contains: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut inherits: HashMap<u32, u32> = HashMap::new();
        for r in &g.relations {
            match r.kind {
                RelationKind::Contains => contains.entry(r.from).or_default().push(r.to),
                RelationKind::Inherits => {
                    inherits.insert(r.from, r.to);
                }
                _ => {}
            }
        }
        Self {
            by_fqn,
            idx_of,
            contains,
            inherits,
        }
    }
}

// ---------------------------------------------------------------------------
// Fingerprints
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Fingerprint {
    parent_fqn: Option<String>,
    module: String,
    member_names: HashSet<String>,
    member_types: HashSet<String>,
}

fn compute_fingerprints(
    g: &SymbolGraph,
    idx: &GraphIndex,
    config: &DiffConfig,
) -> HashMap<u32, Fingerprint> {
    let mut out = HashMap::new();
    for sym in &g.symbols {
        if !matches!(sym.kind, SymbolKind::Class | SymbolKind::Struct) {
            continue;
        }
        let parent_fqn = idx
            .inherits
            .get(&sym.local_id)
            .and_then(|p| idx.idx_of.get(p))
            .map(|&p_idx| g.symbols[p_idx].fqn.clone());
        let mut member_names = HashSet::new();
        let mut member_types = HashSet::new();
        if let Some(children) = idx.contains.get(&sym.local_id) {
            for &c_local in children {
                let Some(&c_idx) = idx.idx_of.get(&c_local) else {
                    continue;
                };
                let child = &g.symbols[c_idx];
                if !matches!(child.kind, SymbolKind::Field | SymbolKind::Function) {
                    continue;
                }
                // Honor the ignore-prefix list.
                if config
                    .fingerprint_ignore_prefixes
                    .iter()
                    .any(|p| child.name.starts_with(p))
                {
                    continue;
                }
                member_names.insert(child.name.clone());
                if let Some(t) = &child.type_ref {
                    member_types.insert(type_ref_canonical(t));
                }
            }
        }
        out.insert(
            sym.local_id,
            Fingerprint {
                parent_fqn,
                module: sym.module.clone(),
                member_names,
                member_types,
            },
        );
    }
    out
}

fn type_ref_canonical(t: &TypeRef) -> String {
    match t {
        TypeRef::Builtin { name, .. } => format!("b:{name}"),
        TypeRef::Unresolved { name, .. } => format!("u:{name}"),
        TypeRef::Local { local_id, .. } => format!("l:{local_id}"),
    }
}

fn jaccard_score(a: &Fingerprint, b: &Fingerprint, config: &DiffConfig) -> f64 {
    let name_score = jaccard(&a.member_names, &b.member_names);
    let type_score = jaccard(&a.member_types, &b.member_types);
    let module_bonus = if a.module == b.module {
        config.same_module_bonus
    } else {
        0.0
    };
    let parent_bonus = if a.parent_fqn == b.parent_fqn {
        0.05
    } else {
        0.0
    };
    name_score * config.member_name_weight
        + type_score * config.member_type_weight
        + module_bonus
        + parent_bonus
}

fn jaccard<T: std::hash::Hash + Eq>(a: &HashSet<T>, b: &HashSet<T>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let inter = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

// ---------------------------------------------------------------------------
// Pass 3 — field-level classification
// ---------------------------------------------------------------------------

fn classify_field_changes(
    head_fqn: &str,
    base: &SymbolGraph,
    base_id: u32,
    base_idx: &GraphIndex,
    head: &SymbolGraph,
    head_id: u32,
    head_idx: &GraphIndex,
) -> Vec<FieldChange> {
    let mut out = Vec::new();

    // Parent-class change.
    let base_parent = base_idx
        .inherits
        .get(&base_id)
        .and_then(|p| base_idx.idx_of.get(p))
        .map(|&pi| base.symbols[pi].fqn.clone());
    let head_parent = head_idx
        .inherits
        .get(&head_id)
        .and_then(|p| head_idx.idx_of.get(p))
        .map(|&pi| head.symbols[pi].fqn.clone());
    if base_parent != head_parent {
        out.push(FieldChange {
            parent_fqn: head_fqn.to_string(),
            change: ChangeKind::ParentClassChanged {
                old_parent: base_parent,
                new_parent: head_parent,
            },
        });
    }

    // Build name -> field lookup for both sides.
    let base_members = members_of(base, base_id, base_idx);
    let head_members = members_of(head, head_id, head_idx);

    let mut head_by_name: HashMap<&str, &Symbol> = HashMap::new();
    for h in &head_members {
        head_by_name.insert(h.name.as_str(), h);
    }
    let mut base_by_name: HashMap<&str, &Symbol> = HashMap::new();
    for b in &base_members {
        base_by_name.insert(b.name.as_str(), b);
    }

    // Compare each base member.
    for b in &base_members {
        let Some(h) = head_by_name.get(b.name.as_str()) else {
            out.push(FieldChange {
                parent_fqn: head_fqn.to_string(),
                change: ChangeKind::FieldRemoved {
                    field: b.name.clone(),
                },
            });
            continue;
        };

        match b.kind {
            SymbolKind::Field => {
                if let (Some(bo), Some(ho)) = (b.offset, h.offset) {
                    if bo != ho {
                        out.push(FieldChange {
                            parent_fqn: head_fqn.to_string(),
                            change: ChangeKind::OffsetChanged {
                                field: b.name.clone(),
                                old_offset: bo,
                                new_offset: ho,
                            },
                        });
                    }
                }
                if let (Some(bs), Some(hs)) = (b.size, h.size) {
                    if bs != hs {
                        out.push(FieldChange {
                            parent_fqn: head_fqn.to_string(),
                            change: ChangeKind::SizeChanged {
                                field: b.name.clone(),
                                old_size: bs,
                                new_size: hs,
                            },
                        });
                    }
                }
                let bt = b.type_ref.as_ref().map(type_ref_canonical);
                let ht = h.type_ref.as_ref().map(type_ref_canonical);
                if bt != ht {
                    out.push(FieldChange {
                        parent_fqn: head_fqn.to_string(),
                        change: ChangeKind::FieldTypeSubstituted {
                            field: b.name.clone(),
                            old_type: bt.unwrap_or_default(),
                            new_type: ht.unwrap_or_default(),
                        },
                    });
                }
            }
            SymbolKind::Function => {
                if let (Some(bs), Some(hs)) = (b.vtable_slot, h.vtable_slot) {
                    if bs != hs {
                        out.push(FieldChange {
                            parent_fqn: head_fqn.to_string(),
                            change: ChangeKind::VtableShift {
                                fn_name: b.name.clone(),
                                old_slot: bs,
                                new_slot: hs,
                            },
                        });
                    }
                }
                let bt = b.type_ref.as_ref().map(type_ref_canonical);
                let ht = h.type_ref.as_ref().map(type_ref_canonical);
                if bt != ht {
                    out.push(FieldChange {
                        parent_fqn: head_fqn.to_string(),
                        change: ChangeKind::FunctionSignatureChanged {
                            fn_name: b.name.clone(),
                            old_return: bt,
                            new_return: ht,
                        },
                    });
                }
            }
            _ => {}
        }
    }

    // Added fields (in head but not base).
    for h in &head_members {
        if !base_by_name.contains_key(h.name.as_str())
            && matches!(h.kind, SymbolKind::Field | SymbolKind::Function)
        {
            out.push(FieldChange {
                parent_fqn: head_fqn.to_string(),
                change: ChangeKind::FieldAdded {
                    field: h.name.clone(),
                },
            });
        }
    }

    out
}

fn members_of<'a>(g: &'a SymbolGraph, parent_local_id: u32, idx: &GraphIndex) -> Vec<&'a Symbol> {
    let mut out = Vec::new();
    if let Some(children) = idx.contains.get(&parent_local_id) {
        for &c in children {
            if let Some(&ci) = idx.idx_of.get(&c) {
                out.push(&g.symbols[ci]);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use atlas_parser_trait::{NullReporter, SdkParser};
    use atlas_parser_ue::Dumper7Parser;
    use std::path::PathBuf;

    use super::*;

    fn synthetic(name: &str) -> SymbolGraph {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.pop();
        p.push("fixtures");
        p.push("synthetic");
        p.push(name);
        Dumper7Parser::new().parse(&p, &NullReporter).unwrap()
    }

    #[test]
    fn exact_match_carries_most_symbols() {
        let v1 = synthetic("tiny-game-v1");
        let v2 = synthetic("tiny-game-v2");
        let d = diff(&v1, &v2, &DiffConfig::default(), &[]);
        // UObject, AActor, APawn, APlayer, EColor all unchanged in name +
        // kind. APawn matches even though its members shifted — pass 1
        // is name-based.
        let exact = d
            .matches
            .iter()
            .filter(|m| matches!(m.method, MatchMethod::Exact))
            .count();
        assert!(exact >= 4, "expected at least 4 exact matches, got {exact}");
    }

    #[test]
    fn added_class_atrap_appears_in_added_list() {
        let v1 = synthetic("tiny-game-v1");
        let v2 = synthetic("tiny-game-v2");
        let d = diff(&v1, &v2, &DiffConfig::default(), &[]);
        assert!(
            d.added.iter().any(|s| s.fqn == "TinyGame.ATrap"),
            "ATrap missing from added: {:?}",
            d.added
        );
    }

    #[test]
    fn rename_aitem_to_apickup_is_detected() {
        let v1 = synthetic("tiny-game-v1");
        let v2 = synthetic("tiny-game-v2");
        let d = diff(&v1, &v2, &DiffConfig::default(), &[]);

        // AItem and APickup share the same parent, same module, and the
        // same member shape, so the fingerprint score lands above the
        // confidence threshold and the engine emits a confident
        // `Fingerprint` match rather than a softer suggestion.
        let confident = d.matches.iter().find(|m| {
            matches!(m.method, MatchMethod::Fingerprint)
                && m.base.fqn == "TinyGame.AItem"
                && m.head.fqn == "TinyGame.APickup"
        });
        let suggested = d
            .renamed_suggestions
            .iter()
            .find(|r| r.base.fqn == "TinyGame.AItem" && r.head.fqn == "TinyGame.APickup");
        assert!(
            confident.is_some() || suggested.is_some(),
            "expected AItem->APickup match or suggestion; matches={:?} suggestions={:?}",
            d.matches,
            d.renamed_suggestions
        );
    }

    #[test]
    fn lower_threshold_produces_suggestion_not_match() {
        let v1 = synthetic("tiny-game-v1");
        let v2 = synthetic("tiny-game-v2");
        let cfg = DiffConfig {
            // Force the suggestion band by lifting the confidence
            // threshold above what AItem->APickup scores (~1.05).
            fingerprint_confidence_threshold: 1.5,
            ..DiffConfig::default()
        };
        let d = diff(&v1, &v2, &cfg, &[]);
        assert!(
            d.renamed_suggestions
                .iter()
                .any(|r| r.base.fqn == "TinyGame.AItem" && r.head.fqn == "TinyGame.APickup"),
            "expected AItem->APickup in suggestions with raised threshold; got: {:?}",
            d.renamed_suggestions
        );
    }

    #[test]
    fn user_override_match_short_circuits_suggestion() {
        let v1 = synthetic("tiny-game-v1");
        let v2 = synthetic("tiny-game-v2");
        let overrides = vec![RenameOverride {
            base_fqn: "TinyGame.AItem".into(),
            head_fqn: "TinyGame.APickup".into(),
            decision: Decision::Match,
        }];
        let d = diff(&v1, &v2, &DiffConfig::default(), &overrides);
        // No longer a suggestion — it's a confirmed match.
        assert!(d.renamed_suggestions.is_empty());
        assert!(
            d.matches
                .iter()
                .any(|m| matches!(m.method, MatchMethod::UserOverride)
                    && m.base.fqn == "TinyGame.AItem"),
            "override match missing"
        );
    }

    #[test]
    fn user_override_reject_excludes_from_suggestions() {
        let v1 = synthetic("tiny-game-v1");
        let v2 = synthetic("tiny-game-v2");
        let overrides = vec![RenameOverride {
            base_fqn: "TinyGame.AItem".into(),
            head_fqn: "TinyGame.APickup".into(),
            decision: Decision::Reject,
        }];
        let d = diff(&v1, &v2, &DiffConfig::default(), &overrides);
        assert!(d
            .renamed_suggestions
            .iter()
            .all(|r| !(r.base.fqn == "TinyGame.AItem" && r.head.fqn == "TinyGame.APickup")));
    }

    #[test]
    fn offset_shift_in_apawn_is_detected() {
        let v1 = synthetic("tiny-game-v1");
        let v2 = synthetic("tiny-game-v2");
        let d = diff(&v1, &v2, &DiffConfig::default(), &[]);
        let speed_offset = d.field_changes.iter().find(|fc| {
            fc.parent_fqn == "TinyGame.APawn"
                && matches!(
                    &fc.change,
                    ChangeKind::OffsetChanged { field, .. } if field == "Speed"
                )
        });
        assert!(
            speed_offset.is_some(),
            "expected APawn.Speed offset shift, got: {:?}",
            d.field_changes
        );
    }

    #[test]
    fn removed_field_lives_field_change() {
        let v1 = synthetic("tiny-game-v1");
        let v2 = synthetic("tiny-game-v2");
        let d = diff(&v1, &v2, &DiffConfig::default(), &[]);
        let removed = d.field_changes.iter().find(|fc| {
            fc.parent_fqn == "TinyGame.APlayer"
                && matches!(&fc.change, ChangeKind::FieldRemoved { field } if field == "Lives")
        });
        assert!(
            removed.is_some(),
            "expected APlayer.Lives removal, got: {:?}",
            d.field_changes
        );
    }

    #[test]
    fn type_substitution_speed_int_to_float() {
        let v1 = synthetic("tiny-game-v1");
        let v2 = synthetic("tiny-game-v2");
        let d = diff(&v1, &v2, &DiffConfig::default(), &[]);
        let sub = d.field_changes.iter().find(|fc| {
            fc.parent_fqn == "TinyGame.APawn"
                && matches!(
                    &fc.change,
                    ChangeKind::FieldTypeSubstituted { field, .. } if field == "Speed"
                )
        });
        assert!(
            sub.is_some(),
            "expected APawn.Speed type substitution, got: {:?}",
            d.field_changes
        );
    }

    #[test]
    fn diff_round_trips_through_json() {
        let v1 = synthetic("tiny-game-v1");
        let v2 = synthetic("tiny-game-v2");
        let d = diff(&v1, &v2, &DiffConfig::default(), &[]);
        let s = serde_json::to_string(&d).unwrap();
        let _back: Diff = serde_json::from_str(&s).unwrap();
    }
}
