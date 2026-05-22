// TinyGame.hpp — TinyGame package, synthetic v2.0.0.
//
// See ../tiny-game-v2/SDK.hpp for the catalogue of changes vs v1.

#pragma once

namespace TinyGame {

// =========================================================================
// Enums
// =========================================================================

// Enum TinyGame.EColor (unchanged from v1)
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
// Class TinyGame.UObject (unchanged from v1)
class UObject {
public:
    int32_t                                            ObjectFlags;                                       // 0x0000(0x0004)
    int32_t                                            InternalIndex;                                     // 0x0004(0x0004)
    uint8_t                                            UnknownData_0008[0x8];                             // 0x0008(0x0008) PADDING
};

// 0x0040 (0x0040 - 0x0010)
// Class TinyGame.AActor : public UObject (unchanged from v1)
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
// CHANGED FROM v1:
//   * Speed changed type int32_t -> float
//   * Ammo and Speed swapped offsets (Speed shifted from 0x40 to 0x44)
class APawn : public AActor {
public:
    int32_t                                            Ammo;                                              // 0x0040(0x0004)
    float                                              Speed;                                             // 0x0044(0x0004)
    uint8_t                                            UnknownData_0048[0x8];                             // 0x0048(0x0008) PADDING

    virtual void Fire();                                                                                  // [0x03] (Virtual)
    virtual int32_t GetAmmo() const;                                                                      // [0x04] (Virtual, Const)
};

// 0x0050 (0x0050 - 0x0040)
// Class TinyGame.APickup : public AActor   (renamed from AItem)
class APickup : public AActor {
public:
    int32_t                                            ItemId;                                            // 0x0040(0x0004)
    int32_t                                            Quantity;                                          // 0x0044(0x0004)
    bool                                               bConsumable;                                       // 0x0048(0x0001)
    uint8_t                                            UnknownData_0049[0x7];                             // 0x0049(0x0007) PADDING
};

// 0x0080 (0x0080 - 0x0050)
// Class TinyGame.APlayer : public APawn
// CHANGED FROM v1:
//   * Lives field removed (padding absorbs the freed 0x0054(0x0004))
class APlayer : public APawn {
public:
    int32_t                                            Score;                                             // 0x0050(0x0004)
    EColor                                             TeamColor;                                         // 0x0054(0x0001)
    uint8_t                                            UnknownData_0055[0x2B];                            // 0x0055(0x002B) PADDING

    virtual void Respawn();                                                                               // [0x05] (Virtual)
    virtual void AddScore(int32_t Amount);                                                                // [0x06] (Virtual)
};

// 0x0048 (0x0048 - 0x0040)
// Class TinyGame.ATrap : public AActor   (NEW in v2)
class ATrap : public AActor {
public:
    float                                              DamagePerTick;                                     // 0x0040(0x0004)
    bool                                               bArmed;                                            // 0x0044(0x0001)
    uint8_t                                            UnknownData_0045[0x3];                             // 0x0045(0x0003) PADDING

    virtual void Trigger();                                                                               // [0x05] (Virtual)
};

}  // namespace TinyGame
