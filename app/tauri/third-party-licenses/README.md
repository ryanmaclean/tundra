# Exact exception license texts

This directory contains the license texts for Tundra's nine exact approved
shipped-product dependency exceptions. `manifest.json` records each package,
version, selected license, source archive checksum, text file, and text hash.
The source files are preserved byte-for-byte from the checksum-verified crate
archives, except `selectors@0.36.1`: its crate archive has an MPL-2.0 source
notice but no license file, so its text is the official Mozilla MPL-2.0 text.

These files cover the exception approvals in `scripts/license_approvals.json`.
They are not a complete third-party notice inventory for every dependency or
for generated or copied build output. A release must verify the final package
contains the relevant texts and complete any other required notices.
