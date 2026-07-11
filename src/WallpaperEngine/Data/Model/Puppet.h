#pragma once

#include <cstdint>
#include <optional>
#include <span>
#include <string>
#include <vector>

#include <glm/mat4x4.hpp>
#include <glm/vec3.hpp>
#include <glm/gtc/quaternion.hpp>

namespace WallpaperEngine::Data::Model {

/**
 * One bone from an MDLS0001 skeleton block.
 *
 * File bytes (little-endian), per Wallpaper Engine's on-disk puppet format:
 *   char     magic[9];   // "MDLS0001\0"
 *   uint32_t sectionEnd; // absolute offset of section end (or size); varies
 *   uint32_t boneCount;
 *   // boneCount records:
 *   struct BoneRecord {
 *       cstring  name;        // bone name (may be empty)
 *       uint32_t unk;
 *       int32_t  parent;      // -1 = root, else parent bone index
 *       uint32_t matrixBytes; // >= 64
 *       float    local[16];   // 4x4 local/bind matrix (row-major in file)
 *       // if matrixBytes > 64: (matrixBytes - 64) extra bytes
 *       cstring  simulation;  // may be empty
 *   };
 *
 * The matrices are D3D row-major / row-vector (translation in the last row).
 * We convert to glm column-vector form (transpose) when loading.
 */
struct PuppetBone {
    std::string name;
    uint32_t type = 0; // file "unk" field (purpose varies)
    int32_t parent = -1;
    glm::mat4 bindMatrix = glm::mat4 (1.0f); // row-major file matrix, transposed to glm
    glm::mat4 bindInverse = glm::mat4 (1.0f);
};

struct PuppetSkeleton {
    uint32_t unknown = 0; // sectionEnd / size field
    std::vector<PuppetBone> bones;
};

/** A single animation keyframe (local transform relative to the bone's rest pose). */
struct PuppetKeyframe {
    float time = 0.0f; // derived from fps * frameIndex at eval time
    glm::vec3 translation = glm::vec3 (0.0f);
    glm::quat rotation = glm::quat (1.0f, 0.0f, 0.0f, 0.0f); // normalized
    glm::vec3 scale = glm::vec3 (1.0f);
};

/** Per-bone track: a contiguous run of keyframes (bone_id from file). */
struct PuppetBoneTrack {
    int32_t boneId = -1;
    std::vector<PuppetKeyframe> keys;
};

/** One animation clip (maps 1:1 onto scene animationlayers[].animation by id). */
struct PuppetClip {
    int32_t id = 0;
    std::string name;
    std::string mode; // "loop" | "mirror" | "single"
    float fps = 0.0f;
    int32_t length = 0; // frame count
    float duration = 0.0f; // seconds (length / fps)
    std::vector<PuppetBoneTrack> tracks;
};

struct PuppetAnimation {
    uint32_t unknown = 0;
    uint32_t clipCount = 0;
    std::vector<PuppetClip> clips;
};

} // namespace WallpaperEngine::Data::Model

