#include "ColorBuilder.h"
#include "VectorBuilder.h"

#include <algorithm>
#include <format>
#include <glm/vec3.hpp>

const WallpaperEngine::Data::Model::Color WallpaperEngine::Data::Builders::ColorBuilder::White
    = WallpaperEngine::Data::Model::Color (1.0f, 1.0f, 1.0f, 1.0f);
const WallpaperEngine::Data::Model::Color WallpaperEngine::Data::Builders::ColorBuilder::Black
    = WallpaperEngine::Data::Model::Color (0.0f, 0.0f, 0.0f, 1.0f);

WallpaperEngine::Data::Model::Color
WallpaperEngine::Data::Builders::ColorBuilder::parse (const std::string& value, float alpha) {
    auto copy = value;

    // replace the actual separators with spaces to normalize them
    if (copy.find (',') != std::string::npos) {
	// replace comma separator with spaces so it's
	std::ranges::replace (copy, ',', ' ');
    }

    // hex colors should be converted to int colors
    if (copy.find ('#') == 0) {
	auto number = copy.substr (1);

	// expand short css notation into the right one
	// support for css notation
	if (number.size () == 3) {
	    number = std::format (
		"{}{}{}{}{}{}{:02x}", number.at (0), number.at (0), number.at (1), number.at (1), number.at (2),
		number.at (2), static_cast<int> (alpha * 255)
	    );
	} else if (number.size () == 4) {
	    number = std::format (
		"{}{}{}{}{}{}{}{}", number.at (0), number.at (0), number.at (1), number.at (1), number.at (2),
		number.at (2), number.at (3), number.at (3)
	    );
	} else if (number.size () != 6 && number.size () != 8) {
	    sLog.exception ("Invalid CSS color notation for ", value);
	}

	// parse hex color
	const auto color = std::stoi (number, nullptr, 16);

	return WallpaperEngine::Data::Model::Color (
	    (color >> 24 & 0xFF) / 255.0f, (color >> 16 & 0xFF) / 255.0f, (color >> 8 & 0xFF) / 255.0f,
	    (color & 0xFF) / 255.0f
	);
    }

    int vectorSize = VectorBuilder::preparseSize (copy);

    if (vectorSize != 3 && vectorSize != 4) {
	throw std::invalid_argument ("Invalid color value");
    }

    // Wallpaper Engine project.json colors are almost always floats in 0..1
    // (e.g. "1 1 1" for white, "0 0 0" for black). Older / hand-written values
    // may use 0..255 integers. Heuristic:
    //  - any RGB component > 1 (or alpha > 1 when present) => 0..255 integers
    //  - otherwise => 0..1 floats (even when written without a '.')
    // Previously, absence of '.' forced the 0..255 path, turning "1 1 1" into ~0.004.
    // Also do NOT inject alpha*255 into the byte-heuristic — that made every 3-comp
    // color look like bytes.
    if (copy.find ('.') == std::string::npos) {
	const auto rgb = VectorBuilder::parse<glm::ivec3> (copy);
	int aInt = 255;
	if (vectorSize == 4) {
	    const auto rgba = VectorBuilder::parse<glm::ivec4> (copy);
	    aInt = rgba.a;
	}
	const bool looksLikeByte = rgb.r > 1 || rgb.g > 1 || rgb.b > 1 || (vectorSize == 4 && aInt > 1);
	if (looksLikeByte) {
	    return {
		rgb.r / 255.0f, rgb.g / 255.0f, rgb.b / 255.0f,
		(vectorSize == 4 ? aInt : static_cast<int> (alpha * 255)) / 255.0f
	    };
	}
	// 0/1 integer triples are floats (WE scheme/clock colors)
	return Model::Color (
	    static_cast<float> (rgb.r), static_cast<float> (rgb.g), static_cast<float> (rgb.b),
	    vectorSize == 4 ? static_cast<float> (aInt) : alpha
	);
    }

    return Model::Color (
	vectorSize == 3 ? glm::vec4 (VectorBuilder::parse<glm::vec3> (copy), alpha)
			: VectorBuilder::parse<glm::vec4> (copy)
    );
}
