# Agent Setup

AI coding agents can use PHPantom for the same code intelligence editors get. The agent starts `phpantom_lsp` over LSP and consults it (diagnostics, go-to-definition, hover types) as it reads and edits PHP, so its changes are grounded in real type information instead of guesses.

> **Fork build.** These are the docs of a fork built for AI agents, with hover on declarations turned on by default. What it adds, how to install it for Claude Code and Codex, and where it differs from these pages: [Fork build for AI agents](fork.md).

## Claude Code

Install the PHPantom plugin from its marketplace:

```
/plugin marketplace add PHPantom-dev/phpantom_claude
/plugin install phpantom@phpantom
```

Restart Claude Code, then open a PHP file. The plugin starts the language server automatically, downloading the correct `phpantom_lsp` binary for your platform on first use (or using one already on your `PATH`). No Docker, PHP, or Rust toolchain is required.

Beyond per-file code intelligence, the plugin puts `phpantom_lsp` on the agent's `PATH` and bundles a skill that tells Claude when to use its project-wide CLI, so it can run `phpantom_lsp analyze` (type-coverage report) and `phpantom_lsp fix` (automated fixes across the codebase) on its own. It also runs `php -l` after each edit to catch syntax errors immediately.

To use a specific binary instead of the downloaded one, set the `PHPANTOM_SERVER_PATH` environment variable to its path, or put `phpantom_lsp` on your `PATH`.

## Opencode

Add this to your `opencode.json` file. It will be invoked when the AI uses the edit tool to edit a PHP file from within Opencode.

```json
{
  "$schema": "https://opencode.ai/config.json",
  "lsp": {
    "php": {
      "command": ["phpantom_lsp", "--stdio"],
      "extensions": [".php"]
    }
  }
}
```

You can add additional extensions to invoke it on if appropriate for your project.

## Hover on declarations

Agents navigate mostly to declarations: workspace symbols, document symbols, go-to-definition, implementation and call hierarchy all hand back the place a symbol is declared. Hover there answers nothing by default, because an editor user is already looking at the declaration. An agent is not: without a hover it has to read the file to learn what PHPantom already knows, such as inherited documentation, resolved types and the owning class.

Turn declaration hover on and hovering a class, interface, trait or enum (its name or its keyword), a method, a property (a promoted constructor property included), a class constant, an enum case, a function or a top-level constant at its declaration, or the name in a `@property` or `@method` tag, shows the same hover as hovering a use of it. It is off by default and never changes what hovering a use shows.

A local `static` or `global` declaration shows the variable's type, the same as hovering a read of the variable.

For Opencode, add the flag to the command:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "lsp": {
    "php": {
      "command": ["phpantom_lsp", "--stdio", "--hover-on-declarations"],
      "extensions": [".php"]
    }
  }
}
```

For a Claude Code plugin, add the flag to the server entry's `args` in its `.lsp.json`:

```json
"args": ["--hover-on-declarations"]
```

Either client can instead pass it per session as an initialization option, with no flag at all:

```json
"initializationOptions": { "hover": { "declarations": true } }
```

The feature is on when the flag or the option turns it on; nothing turns it back off. The option is read only during `initialize`. The browser (WASM) build does not support it: it has no command line and never reads `initializationOptions`, so it is ignored there.
