// TinyGame.hpp — TinyGame package, synthetic v1.0.0.
//
// Format is intentionally simplified relative to the real Dumper-7
// output: one module, classes and enums inline, no separate _classes /
// _structs / _functions split. The parser's grammar lives in
// atlas-parser-ue and is documented inline there.

#pragma once

namespace TinyGame {

// =========================================================================
// Enums
// =========================================================================

// Enum TinyGame.EColor
enum class EColor : uint8_t {
    Red    = 0,
    Green  = 1,
    Blue   = 2,
    Yellow = 3,
};

// =========================================================================
// Classes
// =========================================================================

// 0x0010 (0x0010 - 0x0000)
// Class TinyGame.UObject
class UObject {
public:
    int32_t                                            ObjectFlags;                                       // 0x0000(0x0004)
    int32_t                                            InternalIndex;                                     // 0x0004(0x0004)
    uint8_t                                            UnknownData_0008[0x8];                             // 0x0008(0x0008) PADDING
};

// 0x0040 (0x0040 - 0x0010)
// Class TinyGame.AActor : public UObject
class AActor : public UObject {
public:
    float                                              X;                                                 // 0x0010(0x0004)
    float                                              Y;                                                 // 0x0014(0x0004)
    float                                              Z;                                                 // 0x0018(0x0004)
    int32_t                                            Health;                                            // 0x001C(0x0004)
    bool                                               bAlive;                                            // 0x0020(0x0001)
    uint8_t                                            UnknownData_0021[0x1F];                            // 0x0021(0x001F) PADDING

    virtual void Tick(float DeltaTime);                                                                   // [0x00] (Virtual)
    virtual void BeginPlay();                                                                             // [0x01] (Virtual)
    virtual void EndPlay(EColor reason);                                                                  // [0x02] (Virtual)
};

// 0x0050 (0x0050 - 0x0040)
// Class TinyGame.APawn : public AActor
class APawn : public AActor {
public:
    int32_t                                            Speed;                                             // 0x0040(0x0004)
    int32_t                                            Ammo;                                             // 0x0044(0x0004)
    uint8_t                                            UnknownData_0048[0x8];                             // 0x0048(0x0008) PADDING

    virtual void Fire();                                                                                  // [0x03] (Virtual)
    virtual int32_t GetAmmo() const;                                                                      // [0x04] (Virtual, Const)
};

// 0x0050 (0x0050 - 0x0040)
// Class TinyGame.AItem : public AActor
class AItem : public AActor {
public:
    int32_t                                            ItemId;                                            // 0x0040(0x0004)
    int32_t                                            Quantity;                                          // 0x0044(0x0004)
    bool                                               bConsumable;                                       // 0x0048(0x0001)
    uint8_t                                            UnknownData_0049[0x7];                             // 0x0049(0x0007) PADDING
};

// 0x0080 (0x0080 - 0x0050)
// Class TinyGame.APlayer : public APawn
class APlayer : public APawn {
public:
    int32_t                                            Score;                                             // 0x0050(0x0004)
    int32_t                                            Lives;                                             // 0x0054(0x0004)
    EColor                                             TeamColor;                                         // 0x0058(0x0001)
    uint8_t                                            UnknownData_0059[0x27];                            // 0x0059(0x0027) PADDING

    virtual void Respawn();                                                                               // [0x05] (Virtual)
    virtual void AddScore(int32_t Amount);                                                                // [0x06] (Virtual)
};

}  // namespace TinyGame
