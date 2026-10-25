#!/usr/bin/env nu

# Source-only candidate for a hosted Linux x86-64 job. Do not run on a laptop.
# Inputs are the exact PR116 MVP/EH native receipts from one pinned Tundra head.
def main [source_root: string, mvp_receipt: string, eh_receipt: string, output_dir: string, receipt_dir: string] {
    let source = ($source_root | path expand)
    let mvp = ($mvp_receipt | path expand)
    let eh = ($eh_receipt | path expand)
    let output = ($output_dir | path expand)
    let receipt = ($receipt_dir | path expand)
    let commit = (^git -C $source rev-parse HEAD | str trim)
    if $commit != "f955138e5218942fb3fb097a8fdcb725222406ff" { error make {msg: "DuckDB source commit mismatch"} }
    let lock = (^sha256sum $"($source)/yarn.lock" | split row " " | first)
    if $lock != "c92706a424636737917774117cbe6e6ce04adfe38996ed6c34f45ab7ff31b180" { error make {msg: "Tagged yarn.lock mismatch"} }
    let package = (open $"($source)/packages/duckdb-wasm/package.json")
    if $package.version != "1.11.0" { error make {msg: "Tagged package version changed"} }
    if $nu.os-info.name != "linux" { error make {msg: "Hosted Linux required"} }
    if (^uname -m | str trim) != "x86_64" { error make {msg: "Hosted x86-64 required"} }
    if ($output | path exists) or ($receipt | path exists) { error make {msg: "Output and receipt directories must be absent"} }
    mkdir $output
    mkdir $receipt
    let stage = $"($receipt)/stage"
    mkdir $stage
    let node_modules = $"($source)/node_modules"
    if ($node_modules | path exists) { error make {msg: "Source node_modules must be absent"} }
    mkdir $node_modules

    # Fixed tagged-lock and PR115 legal-closure archives. No npm/yarn install.
    let packages = [
        {name: "apache-arrow-13.0.0", url: "https://registry.npmjs.org/apache-arrow/-/apache-arrow-13.0.0.tgz", sha512: "de0bc25f41836b05b3e8a14d0b6f29eb953ecc687f2d9e9934a58dbbbe0de824252b3c5ea161e98b80a0110b2045210cbb2ac82178b511adacca36bb770707bf", dir: "apache-arrow"}
        {name: "flatbuffers-23.5.26", url: "https://registry.npmjs.org/flatbuffers/-/flatbuffers-23.5.26.tgz", sha512: "bc4f9223dbeb243c22d68113b5320595d0bfa3d1ac54a44cface842f49d08315e5615d557384e4df4863e3118809fb489ca40a8f5177ba9da7f146c85686c849", dir: "flatbuffers"}
        {name: "tslib-2.6.2", url: "https://registry.npmjs.org/tslib/-/tslib-2.6.2.tgz", sha512: "0046311fdde31853e7fdada2540c16f3b56e508911d45554281efb370305ee70530e40ebad3fc7a6dfc8ac2274417856dbb8d304371fe5963bc3a462a93330d9", dir: "tslib"}
        {name: "js-sha256-0.9.0", url: "https://registry.npmjs.org/js-sha256/-/js-sha256-0.9.0.tgz", sha512: "b206b730787db2040ddbea49f55619fb52cfc173b1b812410399eb479fe87cf7ee889044da19e3b1a37cb1ef09ce73a618b376a78f4f7b953fb6d69f72cfdaa4", dir: "js-sha256"}
        {name: "wasm-feature-detect-1.5.1", url: "https://registry.npmjs.org/wasm-feature-detect/-/wasm-feature-detect-1.5.1.tgz", sha512: "187af6dea9ae7a13571d8e3dd36fe127a115e6c500348242dd1ff231f43b8560e0de77e195c25f9c82fde91da88692306bad9aada37a68de1b2f3cf3ab919792", dir: "wasm-feature-detect"}
        {name: "web-worker-1.2.0", url: "https://registry.npmjs.org/web-worker/-/web-worker-1.2.0.tgz", sha512: "3e0177e356afceacb1eb49de13d0c3f974b6e8c30d328510473f4d399c16df69cf42b17aa7bedfd61b5c9e3052115f0118c299d7a721a2a506e21c88d07c7b98", dir: "web-worker"}
        {name: "esbuild-linux-x64-0.19.12", url: "https://registry.npmjs.org/@esbuild/linux-x64/-/linux-x64-0.19.12.tgz", sha512: "07bd60d50a717f006f36b7f225d5437b17a70c8b750a20cdd532172db84ec342a127313bf0a205197e8e27d32bb42d283aa3167fed31a29e2a114f09ac94f00a", dir: "esbuild"}
    ]
    "" | save $"($receipt)/archives.sha512"
    for item in $packages {
        let archive = $"($stage)/($item.name).tgz"
        let destination = if $item.dir == "esbuild" { $"($stage)/esbuild" } else { $"($node_modules)/($item.dir)" }
        mkdir $destination
        ^curl --fail --silent --show-error --location --max-time 90 $item.url -o $archive
        if $env.LAST_EXIT_CODE != 0 { error make {msg: $"Archive download failed: ($item.name)"} }
        let actual = (^sha512sum $archive | split row " " | first)
        if $actual != $item.sha512 { error make {msg: $"Archive hash mismatch: ($item.name)"} }
        ^tar -xzf $archive --strip-components=1 -C $destination
        if $env.LAST_EXIT_CODE != 0 { error make {msg: $"Archive extraction failed: ($item.name)"} }
        $"($item.sha512)  ($item.name)\n" | save --append $"($receipt)/archives.sha512"
    }

    # PR116's native job retains raw Emscripten glue; reproduce tagged
    # wasm_build_lib.sh's filename rewrite before esbuild resolves these imports.
    let bindings = $"($source)/packages/duckdb-wasm/src/bindings"
    for variant in [mvp eh] {
        let native = if $variant == "mvp" { $mvp } else { $eh }
        let expected = if $variant == "mvp" { "7cf852dcc3c399f34330072a422679f2a5b7fc58c5839d3de0035fc68d9b7791" } else { "11994d2ec1a2703219a5d0e26eb764d8a19a9b413889739e914f776f8c71bb6c" }
        let expected_glue = if $variant == "mvp" { "169608644b833fef6b1b9d1c3a9bf21e927c7439f54acf10379259012a2ea9dc" } else { "fd610ecd2c58944726dc473ef8babc400581ae07df2a805b67f29a14b38695d3" }
        let wasm = $"($native)/duckdb-($variant).wasm"
        let glue = $"($native)/duckdb-($variant).js"
        if not ($wasm | path exists) or not ($glue | path exists) { error make {msg: $"Missing PR116 native output: ($variant)"} }
        let toolchain = (open --raw $"($native)/toolchain.txt")
        if not ($toolchain | str contains "tundra_head=214fbae44466fb854cdebfa54e5d6e47ee5088de") { error make {msg: $"PR116 Tundra head mismatch: ($variant)"} }
        if not ($toolchain | str contains "upstream_head=f955138e5218942fb3fb097a8fdcb725222406ff") { error make {msg: $"PR116 upstream head mismatch: ($variant)"} }
        let actual = (^sha256sum $wasm | split row " " | first)
        if $actual != $expected { error make {msg: $"PR116 Wasm hash mismatch: ($variant)"} }
        let actual_glue = (^sha256sum $glue | split row " " | first)
        if $actual_glue != $expected_glue { error make {msg: $"PR116 generated glue hash mismatch: ($variant)"} }
        let raw = (open --raw $glue)
        if not ($raw | str contains 'duckdb_wasm.wasm') { error make {msg: $"Unexpected Emscripten glue: ($variant)"} }
        $raw | str replace --all 'duckdb_wasm.wasm' $"./duckdb-($variant).wasm" | save $"($bindings)/duckdb-($variant).js"
        cp $wasm $"($bindings)/duckdb-($variant).wasm"
        cp $wasm $"($output)/duckdb-($variant).wasm"
    }

    let esbuild = $"($stage)/esbuild/bin/esbuild"
    let targets = 'chrome64,edge79,firefox62,safari11.1'
    let define_browser = '--define:process.release.name="browser"'
    let package_dir = $"($source)/packages/duckdb-wasm"
    # Export only the bridge API. This omits upstream's CDN convenience helper.
    let entry = $"($receipt)/tundra-browser-entry.ts"
    $"export { AsyncDuckDB } from '($package_dir)/src/parallel/async_bindings';\nexport { ConsoleLogger } from '($package_dir)/src/log';\nexport { selectBundle } from '($package_dir)/src/platform';\n" | save $entry
    ^$esbuild $entry --bundle --platform=browser --format=esm $"--target=($targets)" --minify --sourcemap=external --external:module $define_browser $"--metafile=($receipt)/browser-metafile.json" $"--outfile=($output)/duckdb-browser.mjs"
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "Browser entry bundle failed"} }
    for variant in [mvp eh] {
        ^$esbuild $"($package_dir)/src/targets/duckdb-browser-($variant).worker.ts" --bundle --platform=browser --format=iife --global-name=duckdb $"--target=($targets)" --minify --sourcemap=external --external:module $define_browser $"--metafile=($receipt)/worker-($variant)-metafile.json" $"--outfile=($output)/duckdb-browser-($variant).worker.js"
        if $env.LAST_EXIT_CODE != 0 { error make {msg: $"Worker bundle failed: ($variant)"} }
    }
    for map in [duckdb-browser.mjs.map duckdb-browser-mvp.worker.js.map duckdb-browser-eh.worker.js.map] {
        mv $"($output)/($map)" $"($receipt)/($map)"
    }
    let files = (glob $"($output)/*" | sort)
    if ($files | length) != 5 { error make {msg: "Expected exactly three JS and two Wasm outputs"} }
    for file in (glob $"($output)/*.js" | append $"($output)/duckdb-browser.mjs") {
        let js = (open --raw $file)
        if ($js | str contains 'cdn.jsdelivr.net') or ($js | str contains 'unpkg.com') { error make {msg: $"Remote CDN literal in ($file)"} }
    }
    ^sha256sum ...$files | save $"($receipt)/five-output.sha256"
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "Output hashing failed"} }
    {source_commit: $commit, tagged_package_version: $package.version, native_wasm_mvp_sha256: "7cf852dcc3c399f34330072a422679f2a5b7fc58c5839d3de0035fc68d9b7791", native_wasm_eh_sha256: "11994d2ec1a2703219a5d0e26eb764d8a19a9b413889739e914f776f8c71bb6c", coi_built: false, admission: "NO"} | to json | save $"($receipt)/candidate.json"
}
