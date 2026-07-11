# MDL Files

Wallpaper Engine puppet models (2D deformable characters / props). A single
`*_puppet.mdl` file concatenates three sections:

| Magic | Role |
|---|---|
| `MDLV####` | Mesh (positions, blend indices/weights, UVs, indices) |
| `MDLS0001` | Skeleton / bind pose |
| `MDLA####` | Animation clips (not fully reversed yet) |

Workshop puppets observed so far are almost always **`MDLV0013` + `MDLS0001`**.

## MDLV (mesh)

Header is 9 bytes: `"MDLV####\0"`. Vertex layout for MDLV0013 (52-byte stride):

```
VECTOR3 position;        // 12
DWORD   blendindices[4]; // 16
VECTOR4 blendweight;     // 16
VECTOR2 uv;              // 8
= 52 bytes per vertex
```

Indices are `uint16` triangles. The mesh block sits between the MDLV header
(plus a small variable preamble) and the MDLS section; the engine probes common
strides (52/64/80/48) to locate `vertexBytes` / `indexBytes`.

## MDLS (skeleton) — reversed (Phase 1)

```
// little-endian
char     magic[9];          // "MDLS0001\0"
uint32_t unknown;           // not section size (value varies; purpose TBD)
uint32_t boneCount;
// boneCount records of 78 bytes each:
struct BoneRecord {
    uint8_t  tmp;           // always 0
    uint32_t type;          // 0 or 1 observed
    int32_t  parent;        // -1 = root, else parent bone index
    uint32_t dataLen;       // always 64
    float    matrix[16];    // 4x4 bind pose, D3D row-major / row-vector
                            // (translation in the last row)
    uint8_t  pad;           // always 0
};
// total size = 17 + boneCount * 78
```

No bone-name strings are present in MDLS for the workshop files tested
(pendulum / lamp / body / hand / girl).

### Sanity checks used by the parser

1. `17 + boneCount * 78 == sectionSize` (MDLS → MDLA).
2. Parent indices form a DAG (in range, no cycles).
3. `bindMatrix * bindInverse ≈ I` after converting the file matrix to glm
   column-vector form (`transpose` of the stored row-major floats, then
   `inverse`).

## MDLA (animation) — reversed (Phase 2)

A `.mdl` file may embed several `MDLA####` sections (one per animation clip the
model can play). Each section is a **clip**; the scene's `animationlayers[].animation`
field is the **clip id within that object's `.mdl`**, which maps 1:1 onto the
`id` stored in the MDLA header (verified across all 6 tested puppets: 259's four
layers `[66,260,81,129]` equal its four MDLA clip ids; 273's `319`/`384` equal
its two).

```c
// little-endian
char     magic[9];          // "MDLA0001\0"
uint32_t unknown;           // varies (per-model constant)
uint32_t clipCount;         // number of clips in this section

// repeated clipCount times:
struct Clip {
    uint32_t id;            // clip id (== animationlayers[].animation)
    cstring  name;          // NUL-terminated (e.g. "head tilting")
    cstring  extra;         // NUL-terminated (e.g. "mirror" / "loop")
    // --- per-clip header of VARIABLE length between the strings and the data:
    //     we do NOT trust fixed offsets. Observed fields include a u32 boneCount,
    //     a u32 unknown, a u32 frameCount (or byteCount), and sometimes 1-7 bytes
    //     of padding. boneCount is taken from the already-parsed MDLS skeleton.
    // boneCount tracks, each a contiguous run of keyframe blocks:
    struct BoneTrack {
        // perBoneFrames keyframe blocks of exactly 36 bytes:
        struct Keyframe {
            char   name[2]; // 2-char bone token, e.g. "B0".."B5", "tay", "HB"...
            uint8_t pad[2]; // padding
            float  v[8];    // [time, tx, ty, tz, qx, qy, qz, qw]
        };                  // 2 + 2 + 8*4 = 36 bytes
    } tracks[boneCount];
};
```

### Keyframe semantics (validated, 2026-07-11)

- **`v[0] = time`**, `v[1..3] = translation (x,y,z)`, `v[4..7] = quaternion (x,y,z,w)`.
- **Scale is always 1** (not stored) — 2D puppets.
- Keyframes are **local deltas from the rest pose**, not absolute matrices:
  `world(t) = restWorld * delta(t)` where `restWorld = bone.bindMatrix` and
  `delta = composeLocal(time-interpolated keyframe)`. A zero/near-zero keyframe
  therefore reproduces the rest pose.
- Idle clips store a **zero quaternion `(0,0,0,0)`** → treated as identity; the
  engine normalizes any non-zero quaternion before `mat4_cast`.
- Blocks are **bone-packed**: all `perBoneFrames` frames of bone 0, then bone 1,
  … Keyframes within a bone are ordered by increasing `time`. We locate the first
  block by finding a 2-char token that recurs at stride 36, then derive
  `perBoneFrames` from the first name change.

### Sanity checks used by the parser

1. `t=0` (idle clip) evaluated per bone must match `bindMatrix` (Phase 2 DoD:
   `maxErr < 1e-3`). Confirmed `maxErr ≈ 9.4e-5` on the City girl asset.
2. The animated clip must deviate from rest at mid-duration (confirms the data
   is real motion, not all-zeros).

## animationlayers mapping (scene.json / project.json)

The `animationlayers` array inside a scene references clips by the `.mdl`-local
`animation` id. The engine picks the clip `id == animationlayers[i].animation`
from that object's parsed `MDLA` clips. (See `CImage` / `PuppetParser::parseAnimation`.)

## 010 template (historical / incomplete)

The older 010-editor sketch below is partially outdated for MDLS (it assumed
variable-length float blobs). Prefer the fixed 78-byte layout above for
`MDLS0001`.

```
// mdlv => vertices
// mdls => skinning / bind pose
// mdla => animation
```
