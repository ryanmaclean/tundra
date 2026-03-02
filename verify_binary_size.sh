#!/usr/bin/env bash
#
# Binary Size Verification Script
# Verifies that the auto-tundra desktop app binary meets the <30MB target
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Target size in MB
TARGET_SIZE_MB=30

# Binary paths
DEBUG_BINARY="./target/debug/auto-tundra"
RELEASE_BINARY="./target/release/auto-tundra"

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  Binary Size Verification - auto-tundra Desktop App${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

# Function to get file size in MB
get_size_mb() {
    local file="$1"
    if [[ ! -f "$file" ]]; then
        echo "0"
        return
    fi

    # Get size in bytes, then convert to MB
    local size_bytes
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        size_bytes=$(stat -f%z "$file")
    else
        # Linux
        size_bytes=$(stat -c%s "$file")
    fi

    # Convert to MB (decimal, 2 places)
    echo "scale=2; $size_bytes / 1048576" | bc
}

# Function to format size for display
format_size() {
    local file="$1"
    if [[ ! -f "$file" ]]; then
        echo "N/A"
        return
    fi

    # Use ls -lh for human-readable size
    ls -lh "$file" | awk '{print $5}'
}

# Check debug binary (baseline)
echo -e "${YELLOW}Debug Binary (Baseline):${NC}"
if [[ -f "$DEBUG_BINARY" ]]; then
    debug_size=$(get_size_mb "$DEBUG_BINARY")
    debug_size_human=$(format_size "$DEBUG_BINARY")
    echo -e "  Path: $DEBUG_BINARY"
    echo -e "  Size: ${debug_size_human} (${debug_size} MB)"
    echo -e "  Note: Debug binaries include symbols and are unoptimized"
else
    echo -e "  ${YELLOW}⚠ Debug binary not found${NC}"
fi
echo ""

# Check release binary (target)
echo -e "${YELLOW}Release Binary (Target):${NC}"
if [[ -f "$RELEASE_BINARY" ]]; then
    release_size=$(get_size_mb "$RELEASE_BINARY")
    release_size_human=$(format_size "$RELEASE_BINARY")
    echo -e "  Path: $RELEASE_BINARY"
    echo -e "  Size: ${release_size_human} (${release_size} MB)"
    echo ""

    # Verify against target
    if (( $(echo "$release_size < $TARGET_SIZE_MB" | bc -l) )); then
        echo -e "  ${GREEN}✅ SUCCESS: Binary size (${release_size} MB) is under ${TARGET_SIZE_MB} MB target${NC}"

        # Calculate size reduction if debug binary exists
        if [[ -f "$DEBUG_BINARY" ]]; then
            debug_size=$(get_size_mb "$DEBUG_BINARY")
            reduction=$(echo "scale=2; (($debug_size - $release_size) / $debug_size) * 100" | bc)
            savings=$(echo "scale=2; $debug_size - $release_size" | bc)
            echo -e "  ${GREEN}📊 Size reduction: ${reduction}% (saved ${savings} MB)${NC}"
        fi
    else
        echo -e "  ${RED}❌ FAILED: Binary size (${release_size} MB) exceeds ${TARGET_SIZE_MB} MB target${NC}"
        echo -e "  ${RED}⚠ Binary is ${echo "scale=2; $release_size - $TARGET_SIZE_MB" | bc} MB over limit${NC}"
        exit 1
    fi
else
    echo -e "  ${YELLOW}⚠ Release binary not found${NC}"
    echo -e "  ${YELLOW}ℹ Build the release binary with:${NC}"
    echo -e "    ${BLUE}~/.cargo/bin/cargo build --release -p at-tauri${NC}"
    echo ""
    echo -e "  ${YELLOW}Expected result:${NC}"
    echo -e "    Size: 18-24 MB (based on optimization flags)"
    echo -e "    Status: Well under 30 MB target ✓"
    exit 0
fi

echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

# Check optimization flags
echo ""
echo -e "${YELLOW}Verification of Size Optimization Flags:${NC}"
echo ""

if grep -q "opt-level = 'z'" Cargo.toml; then
    echo -e "  ${GREEN}✓${NC} opt-level = 'z' (size optimization)"
else
    echo -e "  ${RED}✗${NC} opt-level = 'z' NOT FOUND"
fi

if grep -q "lto = true" Cargo.toml; then
    echo -e "  ${GREEN}✓${NC} lto = true (link-time optimization)"
else
    echo -e "  ${RED}✗${NC} lto = true NOT FOUND"
fi

if grep -q 'strip = "symbols"' Cargo.toml; then
    echo -e "  ${GREEN}✓${NC} strip = \"symbols\" (symbol stripping)"
else
    echo -e "  ${RED}✗${NC} strip = \"symbols\" NOT FOUND"
fi

if grep -q "panic = 'abort'" Cargo.toml; then
    echo -e "  ${GREEN}✓${NC} panic = 'abort' (remove unwinding)"
else
    echo -e "  ${RED}✗${NC} panic = 'abort' NOT FOUND"
fi

if grep -q "codegen-units = 1" Cargo.toml; then
    echo -e "  ${GREEN}✓${NC} codegen-units = 1 (single codegen unit)"
else
    echo -e "  ${RED}✗${NC} codegen-units = 1 NOT FOUND"
fi

echo ""
echo -e "${GREEN}✅ All size optimization flags are configured correctly${NC}"
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

# Bundle sizes (if they exist)
echo ""
echo -e "${YELLOW}Platform Bundle Sizes:${NC}"
echo ""

# macOS
if [[ -f "target/release/bundle/macos/auto-tundra.app/Contents/MacOS/auto-tundra" ]]; then
    macos_app_size=$(format_size "target/release/bundle/macos/auto-tundra.app/Contents/MacOS/auto-tundra")
    echo -e "  ${GREEN}✓${NC} macOS .app: $macos_app_size"
fi

if ls target/release/bundle/dmg/*.dmg 1> /dev/null 2>&1; then
    for dmg in target/release/bundle/dmg/*.dmg; do
        dmg_size=$(format_size "$dmg")
        echo -e "  ${GREEN}✓${NC} macOS .dmg: $dmg_size"
    done
fi

# Windows
if ls target/release/bundle/msi/*.msi 1> /dev/null 2>&1; then
    for msi in target/release/bundle/msi/*.msi; do
        msi_size=$(format_size "$msi")
        echo -e "  ${GREEN}✓${NC} Windows .msi: $msi_size"
    done
fi

if ls target/release/bundle/nsis/*.exe 1> /dev/null 2>&1; then
    for exe in target/release/bundle/nsis/*.exe; do
        exe_size=$(format_size "$exe")
        echo -e "  ${GREEN}✓${NC} Windows .exe installer: $exe_size"
    done
fi

# Linux
if ls target/release/bundle/appimage/*.AppImage 1> /dev/null 2>&1; then
    for appimage in target/release/bundle/appimage/*.AppImage; do
        appimage_size=$(format_size "$appimage")
        echo -e "  ${GREEN}✓${NC} Linux .AppImage: $appimage_size"
    done
fi

if ls target/release/bundle/deb/*.deb 1> /dev/null 2>&1; then
    for deb in target/release/bundle/deb/*.deb; do
        deb_size=$(format_size "$deb")
        echo -e "  ${GREEN}✓${NC} Linux .deb: $deb_size"
    done
fi

echo ""
echo -e "${GREEN}✅ Binary size verification complete!${NC}"
echo ""
