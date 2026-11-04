# PHP symbol metadata sources

PHP sources that more than one integration test module reads, for the
tests of symbol metadata: which docblock belongs to a declaration, the
PHPDoc types and texts a member writes or inherits, and the answers each
route gives for them (hover, the type engine, completion, definition,
signature help, diagnostics).

- **Layout:** `<family>/<name>.php`, one family per subject
  (`attributes/`, `docblocks/`, `inheritance/`). Tests read a file with
  `include_str!`, so it is compiled into the test binary.
- **When a source moves here:** when a second test module reads it.
  Until then it stays an inline Rust string next to its tests. A snippet
  under about 15 lines is copied rather than moved.
- **Never edit a source for one test.** A test that needs another
  anchor appends top-level lines to its own copy
  (`format!("{SRC}$v = MODE;\n")`) or uses a small inline source.
- **Line endings:** `.gitattributes` checks every file under
  `tests/conformance/` out with LF, so byte offsets are the same on every
  platform.
- **Lint:** the CI `php-lint` job runs `php -l` on every file here with
  PHP 8.5, so each source must be valid PHP 8.5.
