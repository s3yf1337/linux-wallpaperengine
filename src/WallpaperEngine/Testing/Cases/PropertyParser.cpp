#include <catch2/catch_test_macros.hpp>
#include <catch2/catch_approx.hpp>

#include "WallpaperEngine/Data/Builders/ColorBuilder.h"
#include "WallpaperEngine/Data/Model/Property.h"
#include "WallpaperEngine/Data/Parsers/PropertyParser.h"

using WallpaperEngine::Data::JSON::JSON;
using WallpaperEngine::Data::Model::DynamicValue;
using WallpaperEngine::Data::Parsers::PropertyParser;

TEST_CASE ("Bool properties without a value default to false") {
    const JSON propertyData = {
	{ "type", "bool" },
	{ "text", "Enabled" },
    };

    const auto property = PropertyParser::parse (propertyData, "enabled");

    REQUIRE (property != nullptr);
    CHECK (property->getType () == DynamicValue::Boolean);
    CHECK_FALSE (property->getBool ());
}

TEST_CASE ("Directory properties are parsed as file-like properties") {
    const JSON propertyData = {
	{ "type", "directory" },
	{ "text", "Folder" },
    };

    const auto property = PropertyParser::parse (propertyData, "folder");

    REQUIRE (property != nullptr);
    CHECK (property->dump ().find ("folder - file") != std::string::npos);
}

TEST_CASE ("Color '1 1 1' without decimals is white (0..1 floats), not 1/255") {
    using WallpaperEngine::Data::Builders::ColorBuilder;
    const auto white = ColorBuilder::parse ("1 1 1");
    CHECK (white.r == Catch::Approx (1.0f));
    CHECK (white.g == Catch::Approx (1.0f));
    CHECK (white.b == Catch::Approx (1.0f));

    const auto black = ColorBuilder::parse ("0 0 0");
    CHECK (black.r == Catch::Approx (0.0f));
    CHECK (black.g == Catch::Approx (0.0f));
    CHECK (black.b == Catch::Approx (0.0f));

    // Real 0..255 triples still work
    const auto mid = ColorBuilder::parse ("128 64 0");
    CHECK (mid.r == Catch::Approx (128.0f / 255.0f).margin (0.01f));
    CHECK (mid.g == Catch::Approx (64.0f / 255.0f).margin (0.01f));
    CHECK (mid.b == Catch::Approx (0.0f));
}
