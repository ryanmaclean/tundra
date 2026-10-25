#!/usr/bin/env nu

# Stage a complete, pinned browser DuckDB runtime before Trunk processes index.html.
# The caller supplies an output directory under app/leptos-ui and a receipt directory.
def main [output_dir: string, receipt_dir: string] {
    let stage = $"($receipt_dir)/duckdb-stage"
    mkdir $stage
    mkdir $output_dir
    let packages = [
        {name: "duckdb-wasm-1.28.0", url: "https://registry.npmjs.org/@duckdb/duckdb-wasm/-/duckdb-wasm-1.28.0.tgz", sha512: "61ab30a449b1af54a9db4fc565b591edb64126a8fac505d7da6e0b2f36019c0d368382b5caba71b015a693143231bac7e801ca85c514a8095a5df03646f69adf", dir: "node_modules/@duckdb/duckdb-wasm"}
        {name: "apache-arrow-13.0.0", url: "https://registry.npmjs.org/apache-arrow/-/apache-arrow-13.0.0.tgz", sha512: "de0bc25f41836b05b3e8a14d0b6f29eb953ecc687f2d9e9934a58dbbbe0de824252b3c5ea161e98b80a0110b2045210cbb2ac82178b511adacca36bb770707bf", dir: "node_modules/apache-arrow"}
        {name: "tslib-2.6.2", url: "https://registry.npmjs.org/tslib/-/tslib-2.6.2.tgz", sha512: "0046311fdde31853e7fdada2540c16f3b56e508911d45554281efb370305ee70530e40ebad3fc7a6dfc8ac2274417856dbb8d304371fe5963bc3a462a93330d9", dir: "node_modules/tslib"}
        {name: "flatbuffers-23.5.26", url: "https://registry.npmjs.org/flatbuffers/-/flatbuffers-23.5.26.tgz", sha512: "bc4f9223dbeb243c22d68113b5320595d0bfa3d1ac54a44cface842f49d08315e5615d557384e4df4863e3118809fb489ca40a8f5177ba9da7f146c85686c849", dir: "node_modules/flatbuffers"}
        {name: "esbuild-linux-x64-0.19.12", url: "https://registry.npmjs.org/@esbuild/linux-x64/-/linux-x64-0.19.12.tgz", sha512: "07bd60d50a717f006f36b7f225d5437b17a70c8b750a20cdd532172db84ec342a127313bf0a205197e8e27d32bb42d283aa3167fed31a29e2a114f09ac94f00a", dir: "esbuild"}
    ]
    let archive_receipt = $"($receipt_dir)/duckdb-source-archives.sha512"
    "" | save --force $archive_receipt
    for package in $packages {
        let archive = $"($stage)/($package.name).tgz"
        let destination = $"($stage)/($package.dir)"
        mkdir $destination
        ^curl --fail --silent --show-error --location --max-time 90 $package.url -o $archive
        if $env.LAST_EXIT_CODE != 0 { error make {msg: $"Download failed: ($package.name)"} }
        let actual = (^sha512sum $archive | str trim | split row ' ' | first)
        if $actual != $package.sha512 { error make {msg: $"Archive integrity mismatch: ($package.name)"} }
        ^tar -xzf $archive --strip-components=1 -C $destination
        if $env.LAST_EXIT_CODE != 0 { error make {msg: $"Extraction failed: ($package.name)"} }
        $"($package.sha512)  ($package.name)\n" | save --append $archive_receipt
    }
    let node_modules = $"($stage)/node_modules"
    if (open $"($node_modules)/@duckdb/duckdb-wasm/package.json" | get license) != "MIT" { error make {msg: "DuckDB license mismatch"} }
    if (open $"($node_modules)/apache-arrow/package.json" | get license) != "Apache-2.0" { error make {msg: "Arrow license mismatch"} }
    if (open $"($node_modules)/tslib/package.json" | get license) != "0BSD" { error make {msg: "tslib license mismatch"} }
    if (open $"($stage)/esbuild/package.json" | get license) != "MIT" { error make {msg: "esbuild license mismatch"} }
    let esbuild = $"($stage)/esbuild/bin/esbuild"
    let entry = $"($node_modules)/@duckdb/duckdb-wasm/dist/duckdb-browser.mjs"
    ^$esbuild $entry --bundle --platform=browser --format=esm --target=es2020 $"--metafile=($receipt_dir)/duckdb-bundle-metafile.json" $"--outfile=($output_dir)/duckdb-browser.mjs"
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "DuckDB browser bundling failed"} }
    for asset in [duckdb-browser-mvp.worker.js duckdb-browser-eh.worker.js duckdb-mvp.wasm duckdb-eh.wasm] {
        cp $"($node_modules)/@duckdb/duckdb-wasm/dist/($asset)" $"($output_dir)/($asset)"
    }
    let bill = (open app/tauri/third-party-licenses/ui/duckdb-runtime-bill.json)
    for selected in $bill.selected_assets {
        let asset = ($selected.path | path basename)
        let actual = (^sha256sum $"($output_dir)/($asset)" | str trim | split row ' ' | first)
        if $actual != $selected.source_sha256 { error make {msg: $"Selected DuckDB asset mismatch: ($asset)"} }
    }
    let output_files = (glob $"($output_dir)/*" | sort)
    if ($output_files | length) != 5 { error make {msg: "Expected exactly five DuckDB runtime assets"} }
    ^sha256sum ...$output_files | save --force $"($receipt_dir)/duckdb-generated-assets.sha256"
    if $env.LAST_EXIT_CODE != 0 { error make {msg: "DuckDB asset hashing failed"} }
}
