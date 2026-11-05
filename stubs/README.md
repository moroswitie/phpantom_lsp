# PHP standard library stubs

Keep this directory and README in the checkout. The build script watches it
for optional local declarations at `jetbrains/phpstorm-stubs/`.

Normally the build downloads the version pinned in `stubs.lock` into Cargo's
build directory. A local tree with `PhpStormStubsMap.php` and a matching
`.commit` marker takes precedence over that cache. `PHPANTOM_STUBS_DIR` can
instead select an external tree, which is embedded as supplied.

Run `scripts/update-stubs.sh` to update the pinned version. To remove local
declarations, remove only `stubs/jetbrains/`, retaining this README.
See [the build instructions](../docs/BUILDING.md) for offline packaging.
