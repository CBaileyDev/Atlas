// Parser internals: explicit-argument functions are clearer than
// passing a god-context. The duplicated `module`/`file`/`builder`/
// `warnings` quartet would be ugly to encapsulate just to please a
// stylistic lint, so allow it locally.
#![allow(clippy::too_many_arguments)]

//! Recursive-descent parser for Dumper-7-style header files.
//!
//! Grammar (informal, simplified):
//!
//! ```text
//! file        := { topLevel }
//! topLevel    := namespace | classDecl | structDecl | enumDecl | other
//! namespace   := "namespace" Ident "{" { topLevel } "}"
//! classDecl   := "class"  Ident [ ":" "public" Ident ] "{" body "}" ";"
//! structDecl  := "struct" Ident [ ":" "public" Ident ] "{" body "}" ";"
//! enumDecl    := "enum" "class" Ident [ ":" Ident ] "{" enumBody "}" ";"
//! body        := { accessLabel | field | method | nested-typedef | other }
//! field       := type Ident [ "[" Number "]" ] ";"   followed-by-comment-with-offset
//! method      := "virtual"? type Ident "(" params ")" ("const")? ";"  followed-by-vtable-comment
//! ```
//!
//! The parser is intentionally line-oriented inside class bodies: after
//! a `field` or `method` declaration, we expect a `// 0xNNNN(0xNNNN)`
//! or `// [0xNN] (Virtual)` comment on the same logical token line.
//! That comment carries the offset, size, and vtable slot.

use atlas_parser_trait::{
    Relation, RelationKind, SourceLoc, Symbol, SymbolFlags, SymbolKind, TypeModifiers, TypeRef,
};
use std::collections::HashMap;

use crate::lexer::{Token, TokenKind};

/// Accumulator passed across files. Each parsed file appends to it.
#[derive(Debug, Default)]
pub(crate) struct GraphBuilder {
    pub symbols: Vec<Symbol>,
    pub relations: Vec<Relation>,
    next_id: u32,
    /// Map from fully qualified name -> local_id, used to resolve type
    /// references after all files have been parsed.
    fqn_index: HashMap<String, u32>,
    /// Pending "unresolved" type-ref fixups: (symbol_idx, child indicator)
    /// recorded so the resolve pass can convert Unresolved → Local when
    /// a matching name appears.
    pending_type_resolutions: Vec<usize>,
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
        if sym.type_ref.is_some() {
            self.pending_type_resolutions.push(self.symbols.len());
        }
        self.symbols.push(sym);
        id
    }

    fn add_relation(&mut self, from: u32, to: u32, kind: RelationKind) {
        self.relations.push(Relation { from, to, kind });
    }

    /// Convert `Unresolved { name }` refs into `Local { local_id }` when
    /// a symbol with a matching name (or fqn) exists. Builtins are left
    /// alone. Called after every file has been parsed.
    pub fn resolve_references(&mut self) {
        let indices = std::mem::take(&mut self.pending_type_resolutions);
        for idx in indices {
            let sym = &mut self.symbols[idx];
            let Some(tref) = sym.type_ref.take() else {
                continue;
            };
            let resolved = resolve_type_ref(tref, &self.fqn_index);
            sym.type_ref = Some(resolved);
        }

        // Build a quick lookup by short name so we can also tag
        // `OfType` relations.
        let by_short: HashMap<String, u32> = self
            .symbols
            .iter()
            .filter(|s| {
                matches!(
                    s.kind,
                    SymbolKind::Class | SymbolKind::Struct | SymbolKind::Enum
                )
            })
            .map(|s| (s.name.clone(), s.local_id))
            .collect();

        // Also tag OfType relations for resolved Local refs.
        let mut to_add = Vec::new();
        for sym in &self.symbols {
            if let Some(TypeRef::Local { local_id, .. }) = &sym.type_ref {
                to_add.push(Relation {
                    from: sym.local_id,
                    to: *local_id,
                    kind: RelationKind::OfType,
                });
            } else if let Some(TypeRef::Unresolved { name, .. }) = &sym.type_ref {
                // One more attempt against the short-name index — picks
                // up cases where the field type was written without the
                // module prefix.
                if let Some(&id) = by_short.get(name) {
                    to_add.push(Relation {
                        from: sym.local_id,
                        to: id,
                        kind: RelationKind::OfType,
                    });
                }
            }
        }
        self.relations.extend(to_add);
    }
}

fn resolve_type_ref(tref: TypeRef, index: &HashMap<String, u32>) -> TypeRef {
    match tref {
        TypeRef::Unresolved { name, modifiers } => {
            // First try as full FQN, then as bare name.
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
// Cursor — a tiny token-stream helper.
// ---------------------------------------------------------------------------

struct Cursor<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.pos)
    }

    fn peek_at(&self, offset: usize) -> Option<&'a Token> {
        self.tokens.get(self.pos + offset)
    }

    fn advance(&mut self) -> Option<&'a Token> {
        let t = self.tokens.get(self.pos);
        if t.is_some() {
            self.pos += 1;
        }
        t
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

    /// Skip forward until the matched balanced `(` `)` or `{` `}` is
    /// closed. Position lands one past the closer. Used to skip over
    /// function bodies and template-arg lists we don't care about.
    fn skip_balanced(&mut self, open: char, close: char) {
        let mut depth: i32 = 0;
        if !self.at_punct(open) {
            return;
        }
        while let Some(t) = self.peek() {
            if t.kind == TokenKind::Punct {
                if t.text == open.to_string() {
                    depth += 1;
                } else if t.text == close.to_string() {
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
// Top-level entry point.
// ---------------------------------------------------------------------------

pub(crate) fn parse_module(
    module: &str,
    file: &str,
    tokens: &[Token],
    builder: &mut GraphBuilder,
) -> Result<u64, String> {
    let mut cur = Cursor::new(tokens);
    let mut warnings = 0u64;

    // Make sure the module symbol itself exists.
    let module_id = builder.alloc_id();
    builder.push_symbol(Symbol {
        local_id: module_id,
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

    parse_until_brace_or_eof(&mut cur, module, file, module_id, builder, &mut warnings);

    builder.resolve_references();
    Ok(warnings)
}

/// Walk tokens at the top of a file or inside a `namespace { ... }`.
/// Stops at end-of-input or when a `}` closes the surrounding scope.
fn parse_until_brace_or_eof(
    cur: &mut Cursor,
    module: &str,
    file: &str,
    parent_id: u32,
    builder: &mut GraphBuilder,
    warnings: &mut u64,
) {
    while let Some(t) = cur.peek() {
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
                    cur.advance();
                    let name = cur
                        .peek()
                        .filter(|n| n.kind == TokenKind::Ident)
                        .map(|n| n.text.clone());
                    if let Some(_n) = name {
                        cur.advance();
                    }
                    if cur.eat_punct('{') {
                        // We treat the namespace name as part of the
                        // FQN if it equals the module, otherwise as a
                        // sub-prefix. For Phase 1 we only flatten one
                        // level — Dumper-7 wraps each module in one
                        // namespace whose name matches the file.
                        parse_until_brace_or_eof(cur, module, file, parent_id, builder, warnings);
                        cur.eat_punct('}');
                        cur.eat_punct(';');
                    }
                    continue;
                }
                "class" | "struct" => {
                    let is_class = t.text == "class";
                    parse_class_or_struct(
                        cur, module, file, is_class, parent_id, builder, warnings,
                    );
                    continue;
                }
                "enum" => {
                    parse_enum(cur, module, file, parent_id, builder, warnings);
                    continue;
                }
                "template" | "typedef" | "using" => {
                    // Skip these top-level constructs entirely. Find ';' or '{...};'
                    cur.advance();
                    skip_until_semi_or_braced(cur);
                    continue;
                }
                _ => {
                    cur.advance();
                    continue;
                }
            }
        }
        cur.advance();
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
                _ => {
                    cur.advance();
                }
            }
        } else {
            cur.advance();
        }
    }
}

// ---------------------------------------------------------------------------
// Class / struct declaration
// ---------------------------------------------------------------------------

fn parse_class_or_struct(
    cur: &mut Cursor,
    module: &str,
    file: &str,
    is_class: bool,
    parent_id: u32,
    builder: &mut GraphBuilder,
    warnings: &mut u64,
) {
    let header_line = cur.peek().map_or(0, |t| t.line);
    cur.advance(); // consume `class`/`struct`

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
    let name = name_tok.text;

    // Optional `: public Parent`
    let mut parent_name: Option<String> = None;
    if cur.eat_punct(':') {
        // skip access specifier (public/private/protected) if present
        let _ = cur.eat_ident("public") || cur.eat_ident("private") || cur.eat_ident("protected");
        if let Some(pt) = cur.peek() {
            if pt.kind == TokenKind::Ident {
                parent_name = Some(pt.text.clone());
                cur.advance();
            }
        }
    }

    // Forward decl: `class Foo;`
    if cur.eat_punct(';') {
        return;
    }

    if !cur.eat_punct('{') {
        // Unexpected — skip to next ';' to recover.
        *warnings += 1;
        skip_until_semi_or_braced(cur);
        return;
    }

    // Allocate the symbol now so its members can point to it.
    let kind = if is_class {
        SymbolKind::Class
    } else {
        SymbolKind::Struct
    };
    let class_id = builder.alloc_id();
    let fqn = format!("{module}.{name}");
    builder.push_symbol(Symbol {
        local_id: class_id,
        fqn: fqn.clone(),
        name: name.clone(),
        kind,
        module: module.to_string(),
        size: None,
        align: None,
        offset: None,
        vtable_slot: None,
        type_ref: None,
        flags: SymbolFlags {
            public: is_class,
            ..Default::default()
        },
        source_loc: Some(SourceLoc {
            file: file.to_string(),
            line: header_line,
        }),
    });
    builder.add_relation(parent_id, class_id, RelationKind::Contains);

    if let Some(pn) = parent_name {
        if let Some(&pid) = builder.fqn_index.get(&format!("{module}.{pn}")) {
            builder.add_relation(class_id, pid, RelationKind::Inherits);
        } else if let Some(sl) = builder
            .symbols
            .last_mut()
            .expect("just pushed")
            .source_loc
            .as_mut()
        {
            // Defer: record parent name in the source_loc.file so the
            // resolve pass can pick it up. Not pretty; it would be
            // cleaner to add a dedicated `pending_parents` queue. Doing
            // the cleaner thing once cross-module parent linkage is
            // actually needed.
            sl.file = format!("{}#parent={pn}", sl.file);
        }
    }

    // Parse body until matching '}'.
    parse_class_body(cur, module, file, class_id, &name, builder, warnings);

    cur.eat_punct('}');
    cur.eat_punct(';');

    // Second pass: resolve parent if we deferred it. We do this by
    // re-walking the symbol's source_loc trick if present, OR — better
    // — checking after the fact by re-reading the symbol. Simpler: do
    // it on resolve_references for the whole graph. We'll handle that
    // at the file boundary, but we can also do it here on the spot if
    // the parent is now known.
    // (Not implementing the parent-deferred fixup beyond the in-graph
    // resolve_references pass for Phase 1.)
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

        // Access specifier: `public:` / `private:` / `protected:`
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
            cur.advance(); // access keyword
            cur.advance(); // ':'
            continue;
        }

        // static UClass* StaticClass(); — skip
        if t.kind == TokenKind::Ident && t.text == "static" {
            skip_until_semi_or_braced(cur);
            continue;
        }

        // Nested class/struct/enum inside a class body — recurse.
        if t.kind == TokenKind::Ident && matches!(t.text.as_str(), "class" | "struct") {
            let is_class = t.text == "class";
            parse_class_or_struct(cur, module, file, is_class, class_id, builder, warnings);
            continue;
        }
        if t.kind == TokenKind::Ident && t.text == "enum" {
            parse_enum(cur, module, file, class_id, builder, warnings);
            continue;
        }

        // Method? Starts with `virtual` keyword or has `(` later on the
        // same line. We'll peek ahead.
        if t.kind == TokenKind::Ident && t.text == "virtual" {
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
            continue;
        }

        // Field or method without `virtual`. Read until ';' or '('
        // whichever comes first to decide.
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

/// Look ahead until we see `;`, `(`, or `}`. If `(` first → method.
/// If `;` first → field. Otherwise unknown.
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
        if i > 64 {
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

    // Parse type — sequence of idents + optional `*`/`&`/`<...>`
    let type_ref = match read_type(cur) {
        Some(t) => t,
        None => {
            *warnings += 1;
            skip_until_semi_or_braced(cur);
            return;
        }
    };

    // Parse name
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

    // Optional array dim
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

    // Optional default initializer `= ...` — skip
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

    // Next token may be a comment that holds the offset/size annotation.
    let mut offset = None;
    let mut size = None;
    let mut flags = access;
    if let Some(c) = cur.peek().cloned() {
        if c.kind == TokenKind::Comment && c.line == name_tok.line {
            cur.advance();
            let (off, sz, extra_flags) = parse_field_annotation(&c.text);
            offset = off;
            size = sz;
            apply_extra_flags(&mut flags, &extra_flags);
        }
    }

    // Apply array dim to type modifiers if present.
    let type_ref = if array_dim.is_some() {
        let mut m = type_ref.modifiers().clone();
        m.array_dim = array_dim;
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

/// Parse the `// 0xNNNN(0xNNNN)` annotation. Returns (offset, size,
/// extra_flag_tags). Robust against extra commentary after the size.
fn parse_field_annotation(comment: &str) -> (Option<u32>, Option<u32>, Vec<String>) {
    // Looking for the first `0xNNNN` then optional `(0xNNNN)`.
    let mut offset = None;
    let mut size = None;
    let bytes = comment.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'0' && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X') {
            let start = i;
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_hexdigit() {
                j += 1;
            }
            let n = u32::from_str_radix(&comment[start + 2..j], 16).ok();
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
    if comment.contains("(Const") || comment.contains(", Const") {
        flags.push("const".into());
    }
    if comment.contains("Edit") {
        flags.push("editable".into());
    }
    (offset, size, flags)
}

fn apply_extra_flags(flags: &mut SymbolFlags, tags: &[String]) {
    for t in tags {
        if t == "const" {
            flags.const_member = true;
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
    flags.public = access.public;

    if cur.eat_ident("virtual") {
        flags.virtual_fn = true;
    }
    if cur.eat_ident("static") {
        flags.static_member = true;
    }

    // Return type
    let return_type = match read_type(cur) {
        Some(t) => t,
        None => {
            *warnings += 1;
            skip_until_semi_or_braced(cur);
            return;
        }
    };

    // Method name
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

    // Params — skip past balanced parens. For Phase 1 we don't extract
    // individual parameters as separate symbols; they're recorded as
    // part of the function signature in the future.
    if !cur.at_punct('(') {
        *warnings += 1;
        skip_until_semi_or_braced(cur);
        return;
    }
    cur.skip_balanced('(', ')');

    // Optional `const` / `override` / `= 0` / `noexcept`
    while let Some(t) = cur.peek().cloned() {
        if t.kind == TokenKind::Ident {
            match t.text.as_str() {
                "const" => {
                    flags.const_member = true;
                    cur.advance();
                }
                "override" => {
                    cur.advance();
                }
                "noexcept" => {
                    cur.advance();
                }
                _ => break,
            }
        } else if t.kind == TokenKind::Punct && t.text == "=" {
            // `= 0` pure virtual
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

    if !cur.eat_punct(';') {
        *warnings += 1;
        skip_until_semi_or_braced(cur);
        return;
    }

    // Vtable-slot comment: // [0xNN] (Virtual)
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
    // Pattern: `[0xNN] (Virtual)` — pull the first 0xNN we see.
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
    parent_id: u32,
    builder: &mut GraphBuilder,
    warnings: &mut u64,
) {
    let header_line = cur.peek().map_or(0, |t| t.line);
    cur.advance(); // 'enum'

    // optional `class`
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
    let enum_name = name_tok.text;

    // optional `: underlying_type` — read and discard
    if cur.eat_punct(':') {
        let _ = read_type(cur);
    }

    if cur.eat_punct(';') {
        return; // forward decl
    }

    if !cur.eat_punct('{') {
        *warnings += 1;
        skip_until_semi_or_braced(cur);
        return;
    }

    let enum_id = builder.alloc_id();
    let enum_fqn = format!("{module}.{enum_name}");
    builder.push_symbol(Symbol {
        local_id: enum_id,
        fqn: enum_fqn,
        name: enum_name.clone(),
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
    builder.add_relation(parent_id, enum_id, RelationKind::Contains);

    // body: { Ident = Number, ... }
    loop {
        // skip leading comments / commas
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
                fqn: format!("{module}.{enum_name}.{value_name}"),
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
        // Anything else — advance to recover.
        cur.advance();
    }
}

// ---------------------------------------------------------------------------
// Type expression reader.
// ---------------------------------------------------------------------------

const BUILTIN_TYPES: &[&str] = &[
    "void", "bool", "char", "int", "long", "short", "float", "double", "size_t", "uint8_t",
    "uint16_t", "uint32_t", "uint64_t", "int8_t", "int16_t", "int32_t", "int64_t", "uint8",
    "uint16", "uint32", "uint64", "int8", "int16", "int32", "int64",
];

fn is_builtin(name: &str) -> bool {
    BUILTIN_TYPES.contains(&name)
}

/// Read a C++-style type expression. Returns `None` if no type was
/// recognized at the current position.
fn read_type(cur: &mut Cursor) -> Option<TypeRef> {
    let mut modifiers = TypeModifiers::default();
    let mut is_const = false;
    let mut base_name: Option<String> = None;

    while let Some(t) = cur.peek().cloned() {
        if t.kind == TokenKind::Ident {
            match t.text.as_str() {
                "const" => {
                    is_const = true;
                    cur.advance();
                    continue;
                }
                "unsigned" | "signed" => {
                    // sign prefix — leave as part of base name
                    if base_name.is_none() {
                        base_name = Some(t.text.clone());
                    } else {
                        let prev = base_name.take().unwrap_or_default();
                        base_name = Some(format!("{prev} {}", t.text));
                    }
                    cur.advance();
                    continue;
                }
                _ => {
                    if base_name.is_some() {
                        break;
                    }
                    base_name = Some(t.text.clone());
                    cur.advance();
                    // Optional template args: TArray<FString>
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
