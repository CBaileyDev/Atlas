// SDK.hpp — Codex Atlas synthetic Dumper-7-style fixture (TinyGame v2.0.0).
// Differences from v1, deliberate to exercise the diff engine:
//   * +1 class:     ATrap : public AActor (added)
//   * −1 field:     APlayer.Lives removed
//   * offset shift: APawn.Speed pushed from 0x40 to 0x44 (Ammo at 0x40 now)
//   * rename:       AItem -> APickup
//   * type substitution: APawn.Speed changed from int32_t to float

#pragma once

#include "TinyGame.hpp"
