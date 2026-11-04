# PocketJS source subset

Source: https://github.com/pocket-nexus/pocketjs
Revision: `becc0e21b678384ec31efa2b4f8d0be3aeefad5f` (2026-11-04 checkout, package version 0.16.0).
Archive: https://codeload.github.com/pocket-nexus/pocketjs/tar.gz/becc0e21b678384ec31efa2b4f8d0be3aeefad5f
SHA-256 of the downloaded main archive: `2faadedaa3e3eb39ec65f1b08728543d61667a4f2011e3a67c4c71b7773661ab`.
The tar archive's global `comment` records the revision above.

Only the nine Rust crates required by MicroTS/raster and their upstream optional/dev dependencies are included. Upstream crate source and embedded tests are unchanged; the root workspace manifest is reduced to these members and dependencies. No QuickJS, ESP-IDF, C/C++, or Pocket3D is built or vendored. MIT license is in LICENSE.

The build-time compiler is the pinned official npm package @pocketjs/framework 0.16.0. Generated Rust/styles are checked in under assets/ui/pocket/gen; firmware builds do not need Node or Bun.
