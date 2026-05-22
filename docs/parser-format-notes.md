# Parser format notes

Working notes for parser implementers. Verified against real fixtures during each phase. Treat anything not yet verified as a guess.

## Dumper-7 (Unreal Engine)

**Status:** *Unverified. Phase 1 STOP #1 requires a real fixture from Carter before locking these assumptions.*

Working hypotheses (to be confirmed against real dump):

- One header file per package (`FortniteGame.hpp`, `Engine.hpp`, etc.).
- A root `SDK.hpp` that includes all the per-package files.
- Each class declared as `class CLASSNAME : public PARENT { ... };` with explicit offset comments per field (`uint8 SomeField; // 0x0040 (size: 0x4)`).
- Virtual functions emit vtable-slot comments.
- Enums emit `enum class EFoo : uint8 { ... };` with explicit values.

Quirks observed in third-party Dumper-7 documentation:
- Padding fields emitted as `UnknownData_XX` — filter these out before fingerprint computation in the diff engine (plan §9 risks).
- Function bodies may include reflection-stub calls that aren't relevant to the symbol graph.

## IL2CPP / Unity

**Status:** Not yet implemented. Phase 5 deliverable. Format notes will land alongside the stub parser.
