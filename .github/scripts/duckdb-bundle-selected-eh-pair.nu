#!/usr/bin/env nu

# Hosted diagnostic: rebuild the EH worker from one exact PR116 native JS.
def main [source_root: string, native_js: string, condition: string, output_dir: string, receipt_dir: string] {
    if not ($condition in [image clean]) { error make {msg: "Unexpected EH condition"} }
    if $nu.os-info.name != "linux" or (^uname -m | str trim) != "x86_64" { error make {msg: "Hosted Linux x86-64 required"} }
    let source = ($source_root | path expand)
    let commit = (^git -C $source rev-parse HEAD | str trim)
    if $commit != "f955138e5218942fb3fb097a8fdcb725222406ff" { error make {msg: "Upstream source mismatch"} }
    let lock = (^sha256sum $"($source)/yarn.lock" | split row " " | first)
    if $lock != "c92706a424636737917774117cbe6e6ce04adfe38996ed6c34f45ab7ff31b180" { error make {msg: "Upstream yarn.lock mismatch"} }
    if (open $"($source)/packages/duckdb-wasm/package.json" | get version) != "1.11.0" { error make {msg: "Upstream package version mismatch"} }
    let output = ($output_dir | path expand)
    let receipt = ($receipt_dir | path expand)
    if ($output | path exists) or ($receipt | path exists) { error make {msg: "Output or receipt already exists"} }
    mkdir $output
    mkdir $receipt
    let stage = $"($receipt)/stage"
    mkdir $stage
    let node_modules = $"($source)/node_modules"
    if ($node_modules | path exists) { error make {msg: "Source node_modules already exists"} }
    mkdir $node_modules
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

    let raw = (open --raw $native_js)
    if ($raw | split row 'duckdb_wasm.wasm' | length) != 2 { error make {msg: "Expected one native Wasm filename"} }
    let actual = (^sha256sum $native_js | split row " " | first)
    if $actual != "fd610ecd2c58944726dc473ef8babc400581ae07df2a805b67f29a14b38695d3" { error make {msg: "PR116 native JS mismatch"} }
    let binding = $"($source)/packages/duckdb-wasm/src/bindings/duckdb-eh.js"
    $raw | str replace --all 'duckdb_wasm.wasm' './duckdb-eh.wasm' | save $binding
    if (^sha256sum $binding | split row " " | first) != "2a45ebf0fbb2113f6bdf6fcd7c61c7d1f14d8edca6fe871831b295132fefa397" { error make {msg: "Worker input rewrite mismatch"} }
    let esbuild = $"($stage)/esbuild/bin/esbuild"
    let entry = $"($source)/packages/duckdb-wasm/src/targets/duckdb-browser-eh.worker.ts"
    ^$esbuild $entry --bundle --platform=browser --format=iife --global-name=duckdb '--target=chrome64,edge79,firefox62,safari11.1' --minify --sourcemap=external --external:module '--define:process.release.name="browser"' $"--metafile=($receipt)/worker-metafile.json" $"--outfile=($output)/duckdb-browser-eh.worker.js"
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "Exact EH worker bundle failed"} }
    mv $"($output)/duckdb-browser-eh.worker.js.map" $"($receipt)/duckdb-browser-eh.worker.js.map"
    let worker = $"($output)/duckdb-browser-eh.worker.js"
    ^sha256sum $native_js $binding $worker | save $"($receipt)/pair-worker.sha256"
    if (^sha256sum $worker | split row " " | first) != "feec83f39202c08eb93e89df8c181bac32d4fb9ffb83a18bc277953e4d2945c4" { error make {msg: "Same-run worker differs from pinned installed worker"} }
    {schema: "tundra-eh-same-run-worker-pair/v1", condition: $condition, upstream_head: $commit, native_js_sha256: $actual, rewritten_js_sha256: "2a45ebf0fbb2113f6bdf6fcd7c61c7d1f14d8edca6fe871831b295132fefa397", worker_sha256: "feec83f39202c08eb93e89df8c181bac32d4fb9ffb83a18bc277953e4d2945c4", product_admission: "NO"} | to json | save $"($receipt)/pair.json"
}
