// Parser internals: explicit-argument functions are clearer than
// passing a god-context. The duplicated `module`/`file`/`builder`/
// `warnings` quartet would be ugly to encapsulate just to please a
// stylistic lint, so allow it locally.
#![allow(clippy::too_many_arguments)]

//! Recursive-descent parser for Dumper-7-style header files.
//!
//! Format invariants (see lib.rs for the full sketch):
//! - All content sits inside `namespace SDK { ... }` or
//!   `namespace SDK::Params { ... }` — the wrapper is **not** a prefix.
//! - Declarations are preceded by a one-line FQN header comment:
//!   `// Class Engine.Actor`
//!   `// Enum CoreUObject.EObjectFlags`
//!   `// ScriptStruct CoreUObject.Vector`
//!   `// Function Engine.Actor.K2_DestroyActor`
//! - Class declarations may carry `final` and/or `alignas(N)`.
//! - Field types frequently use `class FName` / `struct FOO` as a
//!   forward-declaration prefix — both are skipped during type reading.
//! - Method declarations can have an inline body (`{ STATIC_CLASS_IMPL(...) }`)
//!   which we skip; we only need the signature.
//! - Top-level `DUMPER7_ASSERTS_X;` lines are static_assert macro
//!   invocations and are ignored.

use atlas_parser_trait::{
    Relation, RelationKind, SourceLoc, Symbol, SymbolFlags, SymbolKind, TypeModifiers, TypeRef,
};
use std::collections::HashMap;

use crate::lexer::{Token, TokenKind};

#[derive(Debug, Clone)]
struct DeclHeader {
    /// `"Class"`, `"ScriptStruct"`, `"Struct"`, `"Enum"`, `"Function"`.
    kind: &'static str,
    fqn: String,
}

fn parse_decl_comment(text: &str) -> Option<DeclHeader> {
    for kind in ["Class", "ScriptStruct", "Struct", "Enum", "Function"] {
        let prefix = format!("{kind} ");
        if let Some(rest) = text.strip_prefix(&prefix) {
            let fqn = rest.split_whitespace().next().unwrap_or("").to_string();
            if !fqn.is_empty() && fqn.contains('.') {
                return Some(DeclHeader { kind, fqn });
            }
        }
    }
    None
}

/// Accumulator passed across files. Each parsed file appends to it.
#[derive(Debug, Default)]
pub(crate) struct GraphBuilder {
    pub symbols: Vec<Symbol>,
    pub relations: Vec<Relation>,
    next_id: u32,
    fqn_index: HashMap<String, u32>,
    short_name_index: HashMap<String, u32>,
    /// Per-graph file-local symbol indices for type resolution.
    pending_type_resolutions: Vec<usize>,
    /// Per-file "I saw this class with an unresolved parent name" log,
    /// resolved at the end of the graph.
    pending_parents: Vec<(u32, String)>,
}

impl GraphBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn push_symbol(&mut self, sym: Symbol) -> u32 {
        let id = sym.local_id;
        self.fqn_index.insert(sym.fqn.clone(), id);
        if matches!(
            sym.kind,
            SymbolKind::Class | SymbolKind::Struct | SymbolKind::Enum
        ) {
            // For type-ref resolution we also key by the *source* short
            // name (e.g. UAActor) and by the *canonical* short name
            // from the FQN (e.g. Actor). We prefer not to overwrite if
            // there's a collision — first-seen wins. The diff engine
            // can disambiguate via FQN if it matters.
            self.short_name_index.entry(sym.name.clone()).or_insert(id);
            if let Some(short) = sym.fqn.rsplit_once('.').map(|(_, s)| s.to_string()) {
                self.short_name_index.entry(short).or_insert(id);
            }
        }
        if sym.type_ref.is_some() {
            self.pending_type_resolutions.push(self.symbols.len());
        }
        self.symbols.push(sym);
        id
    }

    fn add_relation(&mut self, from: u32, to: u32, kind: RelationKind) {
        self.relations.push(Relation { from, to, kind });
    }

    pub fn resolve_references(&mut self) {
        // 1) Resolve pending parents recorded during class parsing.
        let pending = std::mem::take(&mut self.pending_parents);
        for (child_id, parent_name) in pending {
            if let Some(&parent_id) = self.short_name_index.get(&parent_name) {
                self.relations.push(Relation {
                    from: child_id,
                    to: parent_id,
                    kind: RelationKind::Inherits,
                });
            }
            // Silently drop unresolved parents; the diff engine can
            // surface external-type references later.
        }

        // 2) Walk every pending type_ref and resolve Unresolved -> Local.
        let indices = std::mem::take(&mut self.pending_type_resolutions);
        for idx in indices {
            let sym = &mut self.symbols[idx];
            let Some(tref) = sym.type_ref.take() else {
                continue;
            };
            let resolved = resolve_type_ref(tref, &self.short_name_index);
            sym.type_ref = Some(resolved);
        }

        // 3) Tag OfType relations for resolved Local refs.
        let mut to_add = Vec::new();
        for sym in &self.symbols {
            if let Some(TypeRef::Local { local_id, .. }) = &sym.type_ref {
                to_add.push(Relation {
                    from: sym.local_id,
                    to: *local_id,
                    kind: RelationKind::OfType,
                });
            }
        }
        self.relations.extend(to_add);
    }
}

fn resolve_type_ref(tref: TypeRef, index: &HashMap<String, u32>) -> TypeRef {
    match tref {
        TypeRef::Unresolved { name, modifiers } => {
            if let Some(&id) = index.get(&name) {
                TypeRef::Local {
                    local_id: id,
                    modifiers: resolve_modifiers(modifiers, index),
                }
            } else {
                TypeRef::Unresolved {
                    name,
                    modifiers: resolve_modifiers(modifiers, index),
                }
            }
        }
        TypeRef::Local {
            local_id,
            modifiers,
        } => TypeRef::Local {
            local_id,
            modifiers: resolve_modifiers(modifiers, index),
        },
        TypeRef::Builtin { name, modifiers } => TypeRef::Builtin {
            name,
            modifiers: resolve_modifiers(modifiers, index),
        },
    }
}

fn resolve_modifiers(mut m: TypeModifiers, index: &HashMap<String, u32>) -> TypeModifiers {
    let args = std::mem::take(&mut m.template_args);
    m.template_args = args
        .into_iter()
        .map(|t| resolve_type_ref(t, index))
        .collect();
    m
}

// ---------------------------------------------------------------------------
// Cursor
// ---------------------------------------------------------------------------

struct Cursor<'a> {
    tokens: &'a [Token],
    pos: usize,
    /// Most-recent declaration-header comment (e.g.
    /// `// Class Engine.Actor`). Consumed by parse_class_or_struct,
    /// parse_enum, etc.
    pending_decl: Option<DeclHeader>,
}

impl<'a> Cursor<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            pos: 0,
            pending_decl: None,
        }
    }

    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.pos)
    }

    fn peek_at(&self, offset: usize) -> Option<&'a Token> {
        self.tokens.get(self.pos + offset)
    }

    fn advance(&mut self) -> Option<&'a Token> {
        let t = self.tokens.get(self.pos);
        if let Some(tt) = t {
            if tt.kind == TokenKind::Comment {
                if let Some(h) = parse_decl_comment(&tt.text) {
                    self.pending_decl = Some(h);
                }
            }
            self.pos += 1;
        }
        t
    }

    fn take_pending_decl_for(&mut self, kind: &str) -> Option<DeclHeader> {
        if let Some(h) = &self.pending_decl {
            if h.kind == kind
                || (kind == "Class" && h.kind == "Class")
                || (kind == "Struct" && (h.kind == "Struct" || h.kind == "ScriptStruct"))
                || (kind == "Enum" && h.kind == "Enum")
            {
                return self.pending_decl.take();
            }
        }
        None
    }

    fn at_ident(&self, text: &str) -> bool {
        self.peek()
            .map(|t| t.kind == TokenKind::Ident && t.text == text)
            .unwrap_or(false)
    }

    fn at_punct(&self, text: char) -> bool {
        let needle = text.to_string();
        self.peek()
            .map(|t| t.kind == TokenKind::Punct && t.text == needle)
            .unwrap_or(false)
    }

    fn eat_ident(&mut self, text: &str) -> bool {
        if self.at_ident(text) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn eat_punct(&mut self, text: char) -> bool {
        if self.at_punct(text) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn skip_balanced(&mut self, open: char, close: char) {
        if !self.at_punct(open) {
            return;
        }
        let mut depth: i32 = 0;
        let open_s = open.to_string();
        let close_s = close.to_string();
        while let Some(t) = self.peek() {
            if t.kind == TokenKind::Punct {
                if t.text == open_s {
                    depth += 1;
                } else if t.text == close_s {
                    depth -= 1;
                    if depth == 0 {
                        self.advance();
                        return;
                    }
                }
            }
            self.advance();
        }
    }
}

// ---------------------------------------------------------------------------
// Top-level
// ---------------------------------------------------------------------------

pub(crate) fn parse_file(
    module: &str,
    file: &str,
    tokens: &[Token],
    builder: &mut GraphBuilder,
) -> Result<u64, String> {
    let mut cur = Cursor::new(tokens);
    let mut warnings = 0u64;

    // Ensure the module symbol exists once across the whole graph.
    if !builder.fqn_index.contains_key(module) {
        let id = builder.alloc_id();
        builder.push_symbol(Symbol {
            local_id: id,
            fqn: module.to_string(),
            name: module.to_string(),
            kind: SymbolKind::Module,
            module: module.to_string(),
            size: None,
            align: None,
            offset: None,
            vtable_slot: None,
            type_ref: None,
            flags: SymbolFlags::default(),
            source_loc: Some(SourceLoc {
                file: file.to_string(),
                line: 1,
            }),
        });
    }

    walk(&mut cur, module, file, builder, &mut warnings);
    Ok(warnings)
}

fn module_id(builder: &GraphBuilder, module: &str) -> u32 {
    *builder
        .fqn_index
        .get(module)
        .expect("module symbol pushed at file start")
}

/// Walk top-level statements until end-of-input or a matching `}`.
fn walk(
    cur: &mut Cursor,
    module: &str,
    file: &str,
    builder: &mut GraphBuilder,
    warnings: &mut u64,
) {
    while let Some(t) = cur.peek().cloned() {
        if t.kind == TokenKind::Punct && t.text == "}" {
            return;
        }
        if t.kind == TokenKind::Comment {
            cur.advance();
            continue;
        }
        if t.kind == TokenKind::Ident {
            match t.text.as_str() {
                "namespace" => {
                    consume_namespace(cur, module, file, builder, warnings);
                    continue;
                }
                "class" => {
                    parse_class_or_struct(cur, module, file, true, builder, warnings);
                    continue;
                }
                "struct" => {
                    parse_class_or_struct(cur, module, file, false, builder, warnings);
                    continue;
                }
                "enum" => {
                    parse_enum(cur, module, file, builder, warnings);
                    continue;
                }
                "template" | "typedef" | "using" => {
                    cur.advance();
                    skip_until_semi_or_braced(cur);
                    continue;
                }
                "constexpr" | "static" | "inline" | "extern" => {
                    // Top-level constants / aliases — skip the whole statement.
                    skip_until_semi_or_braced(cur);
                    continue;
                }
                other => {
                    // `IDENT;` at top level is a macro invocation
                    // (DUMPER7_ASSERTS_X, STATIC_CLASS_IMPL, etc.).
                    if cur.peek_at(1).map(|n| n.text.as_str()) == Some(";") {
                        cur.advance();
                        cur.advance();
                        continue;
                    }
                    // Unrecognized — skip one token.
                    let _ = other;
                    cur.advance();
                    continue;
                }
            }
        }
        cur.advance();
    }
}

fn consume_namespace(
    cur: &mut Cursor,
    module: &str,
    file: &str,
    builder: &mut GraphBuilder,
    warnings: &mut u64,
) {
    cur.advance(); // 'namespace'
                   // Eat dotted/scoped name: Ident (:: Ident)*
    loop {
        if let Some(t) = cur.peek().cloned() {
            if t.kind == TokenKind::Ident {
                cur.advance();
                if cur.at_punct(':') && cur.peek_at(1).map(|n| n.text.as_str()) == Some(":") {
                    cur.advance();
                    cur.advance();
                    continue;
                }
                break;
            }
        }
        break;
    }
    if cur.eat_punct('{') {
        walk(cur, module, file, builder, warnings);
        cur.eat_punct('}');
        cur.eat_punct(';');
    }
}

fn skip_until_semi_or_braced(cur: &mut Cursor) {
    while let Some(t) = cur.peek() {
        if t.kind == TokenKind::Punct {
            match t.text.as_str() {
                ";" => {
                    cur.advance();
                    return;
                }
                "{" => {
                    cur.skip_balanced('{', '}');
                    cur.eat_punct(';');
                    return;
                }
                _ => cur.advance(),
            };
        } else {
            cur.advance();
        }
    }
}

// ---------------------------------------------------------------------------
// Class / struct
// ---------------------------------------------------------------------------

fn parse_class_or_struct(
    cur: &mut Cursor,
    module: &str,
    file: &str,
    is_class: bool,
    builder: &mut GraphBuilder,
    warnings: &mut u64,
) {
    let header_line = cur.peek().map_or(0, |t| t.line);
    let mod_id = module_id(builder, module);
    cur.advance(); // class/struct

    // Optional `alignas(N)`
    if cur.at_ident("alignas") {
        cur.advance();
        if cur.at_punct('(') {
            cur.skip_balanced('(', ')');
        }
    }

    // Type name
    let Some(name_tok) = cur.peek().cloned() else {
        *warnings += 1;
        return;
    };
    if name_tok.kind != TokenKind::Ident {
        *warnings += 1;
        return;
    }
    cur.advance();
    let source_name = name_tok.text;

    // Optional `final`
    let _ = cur.eat_ident("final");

    // Optional `: public Parent` (we ignore multi-inheritance)
    let mut parent_name: Option<String> = None;
    if cur.eat_punct(':') {
        let _ = cur.eat_ident("public") || cur.eat_ident("private") || cur.eat_ident("protected");
        let _ = cur.eat_ident("virtual"); // `virtual public`
                                          // Type name (possibly with `class`/`struct` prefix)
        let _ = cur.eat_ident("class") || cur.eat_ident("struct");
        if let Some(pt) = cur.peek().cloned() {
            if pt.kind == TokenKind::Ident {
                parent_name = Some(pt.text.clone());
                cur.advance();
                // Eat any template args on the parent type.
                if cur.at_punct('<') {
                    cur.skip_balanced('<', '>');
                }
            }
        }
    }

    // Forward declaration: `class X;`
    if cur.eat_punct(';') {
        // Consume the pending header if present so we don't carry it.
        let _ = if is_class {
            cur.take_pending_decl_for("Class")
        } else {
            cur.take_pending_decl_for("Struct")
        };
        return;
    }

    if !cur.eat_punct('{') {
        *warnings += 1;
        skip_until_semi_or_braced(cur);
        return;
    }

    let pending = if is_class {
        cur.take_pending_decl_for("Class")
    } else {
        cur.take_pending_decl_for("Struct")
    };
    let fqn = pending
        .as_ref()
        .map(|h| h.fqn.clone())
        .unwrap_or_else(|| format!("{module}.{source_name}"));

    // Allocate the symbol.
    let kind = if is_class {
        SymbolKind::Class
    } else {
        SymbolKind::Struct
    };
    let class_id = builder.alloc_id();
    builder.push_symbol(Symbol {
        local_id: class_id,
        fqn,
        name: source_name.clone(),
        kind,
        module: module.to_string(),
        size: None,
        align: None,
        offset: None,
        vtable_slot: None,
        type_ref: None,
        flags: SymbolFlags {
            public: true,
            ..Default::default()
        },
        source_loc: Some(SourceLoc {
            file: file.to_string(),
            line: header_line,
        }),
    });
    builder.add_relation(mod_id, class_id, RelationKind::Contains);

    if let Some(pn) = parent_name {
        builder.pending_parents.push((class_id, pn));
    }

    parse_class_body(cur, module, file, class_id, &source_name, builder, warnings);

    cur.eat_punct('}');
    cur.eat_punct(';');
}

fn parse_class_body(
    cur: &mut Cursor,
    module: &str,
    file: &str,
    class_id: u32,
    class_name: &str,
    builder: &mut GraphBuilder,
    warnings: &mut u64,
) {
    let mut current_access = SymbolFlags {
        public: true,
        ..Default::default()
    };

    while let Some(t) = cur.peek().cloned() {
        if t.kind == TokenKind::Punct && t.text == "}" {
            return;
        }
        if t.kind == TokenKind::Comment {
            cur.advance();
            continue;
        }

        if t.kind == TokenKind::Ident
            && matches!(t.text.as_str(), "public" | "private" | "protected")
            && cur.peek_at(1).map(|n| n.text.as_str()) == Some(":")
        {
            current_access = match t.text.as_str() {
                "public" => SymbolFlags {
                    public: true,
                    ..Default::default()
                },
                "private" => SymbolFlags {
                    private: true,
                    ..Default::default()
                },
                "protected" => SymbolFlags {
                    protected: true,
                    ..Default::default()
                },
                _ => SymbolFlags::default(),
            };
            cur.advance();
            cur.advance();
            continue;
        }

        if t.kind == TokenKind::Ident && matches!(t.text.as_str(), "class" | "struct") {
            // Decide whether `class X` here is a nested decl or a
            // forward-decl type prefix in a field. We can tell by
            // looking 2 ahead: if there's `{` or `:` after the name,
            // it's a nested declaration; otherwise it's a type prefix.
            let next = cur.peek_at(2).map(|n| n.text.as_str());
            if matches!(next, Some(":") | Some("{")) {
                let is_class = t.text == "class";
                parse_class_or_struct(cur, module, file, is_class, builder, warnings);
                continue;
            }
            // fall through — treated as field type below
        }

        if t.kind == TokenKind::Ident && t.text == "enum" {
            parse_enum(cur, module, file, builder, warnings);
            continue;
        }

        // template <...> — skip the template clause then continue with
        // whatever follows.
        if t.kind == TokenKind::Ident && t.text == "template" {
            cur.advance();
            if cur.at_punct('<') {
                cur.skip_balanced('<', '>');
            }
            continue;
        }

        // `using X = Y;` / `using namespace X;` / `using X::Y;` — skip.
        if t.kind == TokenKind::Ident && (t.text == "using" || t.text == "typedef") {
            cur.advance();
            skip_until_semi_or_braced(cur);
            continue;
        }

        // `friend ...` — skip whole statement.
        if t.kind == TokenKind::Ident && t.text == "friend" {
            cur.advance();
            skip_until_semi_or_braced(cur);
            continue;
        }

        // Macros and one-token bareword statements (`DUMPER7_ASSERTS_X;`).
        if t.kind == TokenKind::Ident
            && cur.peek_at(1).map(|n| n.text.as_str()) == Some(";")
            && t.text
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase())
        {
            cur.advance();
            cur.advance();
            continue;
        }

        // Field or method?
        if t.kind == TokenKind::Ident {
            let look = peek_decl_shape(cur);
            match look {
                DeclShape::Field => {
                    parse_field(
                        cur,
                        module,
                        file,
                        class_id,
                        class_name,
                        current_access,
                        builder,
                        warnings,
                    );
                }
                DeclShape::Method => {
                    parse_method(
                        cur,
                        module,
                        file,
                        class_id,
                        class_name,
                        current_access,
                        builder,
                        warnings,
                    );
                }
                DeclShape::Unknown => {
                    *warnings += 1;
                    skip_until_semi_or_braced(cur);
                }
            }
            continue;
        }

        cur.advance();
    }
}

#[derive(Debug)]
enum DeclShape {
    Field,
    Method,
    Unknown,
}

/// Decide whether the next token-run is a field or method. We scan
/// forward looking for the first `;`, `(`, or `}`. `;` first → field;
/// `(` first → method; `}` first → we walked off the end.
fn peek_decl_shape(cur: &Cursor) -> DeclShape {
    let mut i = 0;
    while let Some(t) = cur.peek_at(i) {
        if t.kind == TokenKind::Punct {
            match t.text.as_str() {
                "(" => return DeclShape::Method,
                ";" => return DeclShape::Field,
                "}" => return DeclShape::Unknown,
                _ => {}
            }
        }
        i += 1;
        if i > 512 {
            return DeclShape::Unknown;
        }
    }
    DeclShape::Unknown
}

// ---------------------------------------------------------------------------
// Field
// ---------------------------------------------------------------------------

fn parse_field(
    cur: &mut Cursor,
    module: &str,
    file: &str,
    class_id: u32,
    class_name: &str,
    access: SymbolFlags,
    builder: &mut GraphBuilder,
    warnings: &mut u64,
) {
    let start_line = cur.peek().map_or(0, |t| t.line);

    // Storage-class modifiers
    let mut is_static = false;
    while let Some(t) = cur.peek().cloned() {
        if t.kind != TokenKind::Ident {
            break;
        }
        match t.text.as_str() {
            "static" => {
                is_static = true;
                cur.advance();
            }
            "inline" | "constexpr" | "mutable" | "volatile" => {
                cur.advance();
            }
            _ => break,
        }
    }

    let type_ref = match read_type(cur) {
        Some(t) => t,
        None => {
            *warnings += 1;
            skip_until_semi_or_braced(cur);
            return;
        }
    };

    let Some(name_tok) = cur.peek().cloned() else {
        *warnings += 1;
        return;
    };
    if name_tok.kind != TokenKind::Ident {
        *warnings += 1;
        skip_until_semi_or_braced(cur);
        return;
    }
    cur.advance();
    let field_name = name_tok.text;

    let mut array_dim: Option<u32> = None;
    if cur.eat_punct('[') {
        if let Some(t) = cur.peek().cloned() {
            if t.kind == TokenKind::Number {
                array_dim = parse_number(&t.text);
                cur.advance();
            }
        }
        let _ = cur.eat_punct(']');
    }

    // Default initializer `= ...` — skip
    if cur.at_punct('=') {
        while let Some(t) = cur.peek() {
            if t.kind == TokenKind::Punct && t.text == ";" {
                break;
            }
            cur.advance();
        }
    }

    if !cur.eat_punct(';') {
        *warnings += 1;
        skip_until_semi_or_braced(cur);
        return;
    }

    let mut offset = None;
    let mut size = None;
    let mut flags = access;
    flags.static_member = is_static;
    if let Some(c) = cur.peek().cloned() {
        if c.kind == TokenKind::Comment && c.line == name_tok.line {
            cur.advance();
            let (off, sz, extra) = parse_field_annotation(&c.text);
            offset = off;
            size = sz;
            apply_extra_flags(&mut flags, &extra);
        }
    }

    let type_ref = if let Some(dim) = array_dim {
        let mut m = type_ref.modifiers().clone();
        m.array_dim = Some(dim);
        match type_ref {
            TypeRef::Local { local_id, .. } => TypeRef::Local {
                local_id,
                modifiers: m,
            },
            TypeRef::Builtin { name, .. } => TypeRef::Builtin { name, modifiers: m },
            TypeRef::Unresolved { name, .. } => TypeRef::Unresolved { name, modifiers: m },
        }
    } else {
        type_ref
    };

    let id = builder.alloc_id();
    builder.push_symbol(Symbol {
        local_id: id,
        fqn: format!("{module}.{class_name}.{field_name}"),
        name: field_name,
        kind: SymbolKind::Field,
        module: module.to_string(),
        size,
        align: None,
        offset,
        vtable_slot: None,
        type_ref: Some(type_ref),
        flags,
        source_loc: Some(SourceLoc {
            file: file.to_string(),
            line: start_line,
        }),
    });
    builder.add_relation(class_id, id, RelationKind::Contains);
}

fn parse_field_annotation(comment: &str) -> (Option<u32>, Option<u32>, Vec<String>) {
    let mut offset = None;
    let mut size = None;
    let bytes = comment.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'0' && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X') {
            let start = i + 2;
            let mut j = start;
            while j < bytes.len() && bytes[j].is_ascii_hexdigit() {
                j += 1;
            }
            let n = u32::from_str_radix(&comment[start..j], 16).ok();
            if offset.is_none() {
                offset = n;
            } else if size.is_none() {
                size = n;
                break;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    let mut flags = Vec::new();
    if comment.contains("Const") {
        flags.push("const".into());
    }
    if comment.contains("Deprecated") {
        flags.push("deprecated".into());
    }
    (offset, size, flags)
}

fn apply_extra_flags(flags: &mut SymbolFlags, tags: &[String]) {
    for t in tags {
        match t.as_str() {
            "const" => flags.const_member = true,
            "deprecated" => flags.deprecated = true,
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Method
// ---------------------------------------------------------------------------

fn parse_method(
    cur: &mut Cursor,
    module: &str,
    file: &str,
    class_id: u32,
    class_name: &str,
    access: SymbolFlags,
    builder: &mut GraphBuilder,
    warnings: &mut u64,
) {
    let start_line = cur.peek().map_or(0, |t| t.line);
    let mut flags = access;

    // Storage / virtual qualifiers, in any order.
    loop {
        if cur.eat_ident("virtual") {
            flags.virtual_fn = true;
        } else if cur.eat_ident("static") {
            flags.static_member = true;
        } else if cur.eat_ident("inline") || cur.eat_ident("constexpr") || cur.eat_ident("explicit")
        {
            // qualifier we don't carry separately
        } else {
            break;
        }
    }

    let return_type = match read_type(cur) {
        Some(t) => t,
        None => {
            *warnings += 1;
            skip_until_semi_or_braced(cur);
            return;
        }
    };

    let Some(name_tok) = cur.peek().cloned() else {
        *warnings += 1;
        return;
    };
    if name_tok.kind != TokenKind::Ident {
        *warnings += 1;
        skip_until_semi_or_braced(cur);
        return;
    }
    cur.advance();
    let method_name = name_tok.text;

    if !cur.at_punct('(') {
        *warnings += 1;
        skip_until_semi_or_braced(cur);
        return;
    }
    cur.skip_balanced('(', ')');

    // Post-signature qualifiers and pure-virtual marker.
    while let Some(t) = cur.peek().cloned() {
        if t.kind == TokenKind::Ident {
            match t.text.as_str() {
                "const" => {
                    flags.const_member = true;
                    cur.advance();
                }
                "override" | "noexcept" | "final" => {
                    cur.advance();
                }
                _ => break,
            }
        } else if t.kind == TokenKind::Punct && t.text == "=" {
            cur.advance();
            if let Some(n) = cur.peek().cloned() {
                if n.kind == TokenKind::Number && n.text == "0" {
                    flags.pure_virtual = true;
                    cur.advance();
                }
            }
        } else {
            break;
        }
    }

    // Method may have an inline body `{ ... }` or end with `;`.
    if cur.at_punct('{') {
        cur.skip_balanced('{', '}');
        // Optional trailing semicolon.
        let _ = cur.eat_punct(';');
    } else if !cur.eat_punct(';') {
        *warnings += 1;
        skip_until_semi_or_braced(cur);
        return;
    }

    let mut vtable_slot = None;
    if let Some(c) = cur.peek().cloned() {
        if c.kind == TokenKind::Comment && c.line == start_line {
            cur.advance();
            vtable_slot = parse_vtable_annotation(&c.text);
        }
    }

    let id = builder.alloc_id();
    builder.push_symbol(Symbol {
        local_id: id,
        fqn: format!("{module}.{class_name}.{method_name}"),
        name: method_name,
        kind: SymbolKind::Function,
        module: module.to_string(),
        size: None,
        align: None,
        offset: None,
        vtable_slot,
        type_ref: Some(return_type),
        flags,
        source_loc: Some(SourceLoc {
            file: file.to_string(),
            line: start_line,
        }),
    });
    builder.add_relation(class_id, id, RelationKind::Contains);
}

fn parse_vtable_annotation(comment: &str) -> Option<u32> {
    let bytes = comment.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'0' && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X') {
            let start = i + 2;
            let mut j = start;
            while j < bytes.len() && bytes[j].is_ascii_hexdigit() {
                j += 1;
            }
            return u32::from_str_radix(&comment[start..j], 16).ok();
        }
        i += 1;
    }
    None
}

// ---------------------------------------------------------------------------
// Enum
// ---------------------------------------------------------------------------

fn parse_enum(
    cur: &mut Cursor,
    module: &str,
    file: &str,
    builder: &mut GraphBuilder,
    warnings: &mut u64,
) {
    let header_line = cur.peek().map_or(0, |t| t.line);
    let mod_id = module_id(builder, module);
    cur.advance(); // 'enum'
    let _ = cur.eat_ident("class");

    let Some(name_tok) = cur.peek().cloned() else {
        *warnings += 1;
        return;
    };
    if name_tok.kind != TokenKind::Ident {
        *warnings += 1;
        return;
    }
    cur.advance();
    let source_name = name_tok.text;

    if cur.eat_punct(':') {
        let _ = read_type(cur);
    }

    if cur.eat_punct(';') {
        let _ = cur.take_pending_decl_for("Enum");
        return;
    }

    if !cur.eat_punct('{') {
        *warnings += 1;
        skip_until_semi_or_braced(cur);
        return;
    }

    let pending = cur.take_pending_decl_for("Enum");
    let fqn = pending
        .as_ref()
        .map(|h| h.fqn.clone())
        .unwrap_or_else(|| format!("{module}.{source_name}"));

    let enum_id = builder.alloc_id();
    builder.push_symbol(Symbol {
        local_id: enum_id,
        fqn,
        name: source_name.clone(),
        kind: SymbolKind::Enum,
        module: module.to_string(),
        size: None,
        align: None,
        offset: None,
        vtable_slot: None,
        type_ref: None,
        flags: SymbolFlags {
            public: true,
            ..Default::default()
        },
        source_loc: Some(SourceLoc {
            file: file.to_string(),
            line: header_line,
        }),
    });
    builder.add_relation(mod_id, enum_id, RelationKind::Contains);

    loop {
        while let Some(t) = cur.peek().cloned() {
            if t.kind == TokenKind::Comment {
                cur.advance();
                continue;
            }
            if t.kind == TokenKind::Punct && t.text == "," {
                cur.advance();
                continue;
            }
            break;
        }
        let Some(t) = cur.peek().cloned() else { break };
        if t.kind == TokenKind::Punct && t.text == "}" {
            cur.advance();
            cur.eat_punct(';');
            break;
        }
        if t.kind == TokenKind::Ident {
            let value_name = t.text.clone();
            let v_line = t.line;
            cur.advance();

            let mut value: Option<u32> = None;
            if cur.eat_punct('=') {
                if let Some(n) = cur.peek().cloned() {
                    if n.kind == TokenKind::Number {
                        value = parse_number(&n.text);
                        cur.advance();
                    }
                }
            }

            let id = builder.alloc_id();
            builder.push_symbol(Symbol {
                local_id: id,
                fqn: format!("{module}.{source_name}.{value_name}"),
                name: value_name,
                kind: SymbolKind::EnumValue,
                module: module.to_string(),
                size: None,
                align: None,
                offset: value,
                vtable_slot: None,
                type_ref: None,
                flags: SymbolFlags::default(),
                source_loc: Some(SourceLoc {
                    file: file.to_string(),
                    line: v_line,
                }),
            });
            builder.add_relation(enum_id, id, RelationKind::Contains);
            continue;
        }
        cur.advance();
    }
}

// ---------------------------------------------------------------------------
// Type expression reader
// ---------------------------------------------------------------------------

const BUILTIN_TYPES: &[&str] = &[
    "void", "bool", "char", "int", "long", "short", "float", "double", "size_t", "uint8_t",
    "uint16_t", "uint32_t", "uint64_t", "int8_t", "int16_t", "int32_t", "int64_t", "uint8",
    "uint16", "uint32", "uint64", "int8", "int16", "int32", "int64", "FString", "FText", "FName",
];

fn is_builtin(name: &str) -> bool {
    BUILTIN_TYPES.contains(&name)
}

fn read_type(cur: &mut Cursor) -> Option<TypeRef> {
    let mut modifiers = TypeModifiers::default();
    let mut is_const = false;
    let mut base_name: Option<String> = None;

    // Eat `class`/`struct`/`enum` forward-decl prefix on types (e.g.
    // `class UFoo*` in Dumper-7 output).
    let _ = cur.eat_ident("class") || cur.eat_ident("struct") || cur.eat_ident("enum");

    while let Some(t) = cur.peek().cloned() {
        if t.kind == TokenKind::Ident {
            match t.text.as_str() {
                "const" => {
                    is_const = true;
                    cur.advance();
                    continue;
                }
                "unsigned" | "signed" => {
                    let prev = base_name.unwrap_or_default();
                    base_name = Some(if prev.is_empty() {
                        t.text.clone()
                    } else {
                        format!("{prev} {}", t.text)
                    });
                    cur.advance();
                    continue;
                }
                _ => {
                    if base_name.is_some() {
                        break;
                    }
                    base_name = Some(t.text.clone());
                    cur.advance();
                    // `Foo::Bar::Baz` namespace-qualified names.
                    while cur.at_punct(':') && cur.peek_at(1).map(|n| n.text.as_str()) == Some(":")
                    {
                        cur.advance();
                        cur.advance();
                        if let Some(n) = cur.peek().cloned() {
                            if n.kind == TokenKind::Ident {
                                let prev = base_name.take().unwrap_or_default();
                                base_name = Some(format!("{prev}::{}", n.text));
                                cur.advance();
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    if cur.at_punct('<') {
                        cur.advance();
                        let args = read_template_args(cur);
                        modifiers.template_args = args;
                        let _ = cur.eat_punct('>');
                    }
                    continue;
                }
            }
        }
        if t.kind == TokenKind::Punct {
            match t.text.as_str() {
                "*" => {
                    modifiers.pointer_depth = modifiers.pointer_depth.saturating_add(1);
                    cur.advance();
                    continue;
                }
                "&" => {
                    modifiers.is_reference = true;
                    cur.advance();
                    continue;
                }
                _ => break,
            }
        }
        break;
    }

    modifiers.is_const = is_const;
    let base = base_name?;
    Some(if is_builtin(&base) {
        TypeRef::Builtin {
            name: base,
            modifiers,
        }
    } else {
        TypeRef::Unresolved {
            name: base,
            modifiers,
        }
    })
}

fn read_template_args(cur: &mut Cursor) -> Vec<TypeRef> {
    let mut args = Vec::new();
    let mut depth: i32 = 0;
    while let Some(t) = read_type(cur) {
        args.push(t);
        if cur.at_punct(',') {
            cur.advance();
            continue;
        }
        if cur.at_punct('>') {
            break;
        }
        if cur.at_punct('<') {
            depth += 1;
            cur.advance();
            continue;
        }
        if depth == 0 {
            break;
        }
        cur.advance();
    }
    args
}

fn parse_number(s: &str) -> Option<u32> {
    if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u32::from_str_radix(rest, 16).ok();
    }
    s.parse::<u32>().ok()
}
