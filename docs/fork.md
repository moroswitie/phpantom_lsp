# Fork build for AI agents

This is a fork of [PHPantom](https://github.com/PHPantom-dev/phpantom_lsp) built for AI coding agents such as Claude Code and Codex. It is not an official PHPantom release. Report problems internally, not to the PHPantom issue tracker.

The fork is upstream PHPantom plus the changes below. Each one is also prepared as a separate pull request for upstream. The release notes of each fork release name the upstream commit it is built on.

## What this build adds

### Hover on declarations, on by default

Agents mostly land on declarations: workspace symbols, document symbols, go-to-definition, implementation and call hierarchy all return the place where a symbol is declared. Upstream, hover there answers nothing, because an editor user is already looking at the declaration. An agent is not, so without a hover it has to read the file to learn what PHPantom already knows: inherited documentation, resolved types, the owning class.

In this build, hovering a declaration shows the same hover as hovering a use of it. This covers:

- a class, interface, trait or enum (its name or its keyword);
- a method, a class constant, an enum case;
- a property, including a promoted constructor property;
- the name in a `@property` or `@method` tag;
- a function or a top-level constant;
- a local `static` or `global` declaration, which shows the variable's type.

Hovering a use shows exactly what it shows upstream.

### Fixes

These fixes apply everywhere, not only at declarations:

- **Open files keep their own declarations.** In a file with unsaved changes, hover and go-to-definition could switch to the saved copy once another file used one of its symbols. They now keep using the client's text.
- **Hover keeps PHP modifiers.** Class and member hovers show `readonly`, `final` and `abstract`, including the class around a constructor or inherited member.
- **Descriptions on constructor-promoted properties.** Hover shows the description from the parameter's own docblock, else from the constructor's `@param` tag.
- **Real properties keep their declarations.** A more specific `@property` tag no longer hides a real property's visibility, native type or documentation. On a property inherited from a parent class or trait, the tag still narrows the type.
- **Magic member descriptions appear in hover.** Members documented with `@property` or `@method` show their description.
- **Parameter descriptions beside vendor `@param` tags.** A `@phpstan-param` or `@psalm-param` tag that only restates the type no longer hides the plain `@param` description, in hover and in signature help.

## Turning it on

There is nothing to turn on. Declaration hover is on as soon as you use this build.

The other pages of these docs describe upstream, where the feature is off by default and turned on with the `--hover-on-declarations` flag or the `{"hover": {"declarations": true}}` initialization option (see [Agent Setup](agent-setup.md#hover-on-declarations)). This build still accepts both, but they change nothing.

It cannot be turned off. If a client needs it off, such as an editor, give that client the official `phpantom_lsp` instead. The install steps below keep the fork binary away from your editors for that reason.

## Installing for Claude Code

The fork uses the official PHPantom plugin and only swaps the binary it starts.

1. Install the plugin:

   ```text
   /plugin marketplace add PHPantom-dev/phpantom_claude
   /plugin install phpantom@phpantom
   ```

2. Download the fork binary into a private directory and point `PHPANTOM_SERVER_PATH` at it. Set `tag` to the release you install.

   **Windows (PowerShell):**

   ```powershell
   $tag = "0.11.1-fork.1"
   $dir = "$env:LOCALAPPDATA\phpantom-fork"
   New-Item -ItemType Directory -Force $dir | Out-Null
   Invoke-WebRequest "https://github.com/moroswitie/phpantom_lsp/releases/download/$tag/phpantom_lsp-x86_64-pc-windows-msvc.zip" -OutFile "$dir\phpantom.zip"
   Expand-Archive "$dir\phpantom.zip" -DestinationPath $dir -Force
   Remove-Item "$dir\phpantom.zip"
   [Environment]::SetEnvironmentVariable("PHPANTOM_SERVER_PATH", "$dir\phpantom_lsp.exe", "User")
   ```

   On ARM Windows, use `aarch64-pc-windows-msvc`.

   **Linux / macOS (bash or zsh):**

   ```bash
   tag="0.11.1-fork.1"
   target="x86_64-unknown-linux-gnu"   # aarch64-unknown-linux-gnu, x86_64-apple-darwin, aarch64-apple-darwin
   dir="$HOME/.local/opt/phpantom-fork"
   mkdir -p "$dir"
   curl -fL "https://github.com/moroswitie/phpantom_lsp/releases/download/$tag/phpantom_lsp-$target.tar.gz" | tar xz -C "$dir"
   xattr -d com.apple.quarantine "$dir/phpantom_lsp" 2>/dev/null || true   # macOS: unsigned build
   echo 'export PHPANTOM_SERVER_PATH="$HOME/.local/opt/phpantom-fork/phpantom_lsp"' >> ~/.bashrc   # zsh: ~/.zshrc
   ```

3. Close every terminal and Claude Code window, then start Claude Code from a new terminal.

Never copy the fork binary into a directory on `PATH`. That is what keeps editors on their own binary.

**Verify:**

1. Ask Claude to run `phpantom_lsp --version` through its Bash tool. The plugin resolves `PHPANTOM_SERVER_PATH` the same way for the language server, so the output must be `phpantom_lsp <tag>`.
2. Ask Claude to use LSP hover on the `class` keyword of any class in the project. It must return the class description, not nothing.

## Installing for Codex and other clients

Codex has no LSP support of its own. It reaches PHPantom through an LSP-to-MCP bridge, an MCP server that starts a language server and passes requests to it. Any bridge works. Configure it the way its own documentation says, with two fork-specific points:

- **Language server command:** the full path to the fork binary with `--stdio`, for example `%LOCALAPPDATA%\phpantom-fork\phpantom_lsp.exe --stdio` on Windows or `~/.local/opt/phpantom-fork/phpantom_lsp --stdio` on Linux and macOS. Download the binary as in step 2 above; the `PHPANTOM_SERVER_PATH` line is only needed for Claude Code, because only the Claude Code plugin reads it. Use the full path, not `phpantom_lsp` on `PATH`, for the same reason as above.
- **No flag and no initialization option.** Declaration hover is already on.

To verify, ask the agent to hover the `class` keyword of any class through the bridge. It must return the class description, not nothing.

The same applies to any other client that starts a language server itself, such as Opencode: put the full path to the fork binary in its command (see [Agent Setup](agent-setup.md#opencode)).

## Updating

1. Close every client that runs the fork server: every Claude Code window, and every Codex session using a bridge. Windows does not let a running `.exe` be overwritten.
2. On Windows, check that no fork server is left running. Match on the path, because an editor may run its own `phpantom_lsp` from somewhere else and must not be stopped:

   ```powershell
   $exe = "$env:LOCALAPPDATA\phpantom-fork\phpantom_lsp.exe"
   Get-Process phpantom_lsp -ErrorAction SilentlyContinue | Where-Object Path -eq $exe | Stop-Process
   ```

   On Linux and macOS no step is needed once the clients are closed.
3. Re-run the download step with the new tag. It overwrites the binary in place.
4. Start the client again and repeat the verify steps. The version must be the new tag.

## Going back to the official build

- **Claude Code on Windows:** `[Environment]::SetEnvironmentVariable("PHPANTOM_SERVER_PATH", $null, "User")`.
- **Claude Code on Linux and macOS:** remove the `export PHPANTOM_SERVER_PATH=…` line from the shell startup file.
- **Bridges and other clients:** point their command back at the official `phpantom_lsp`.

Then install the official `phpantom_lsp` as the plugin's README describes, and restart the client.

## Known limits

- **A promoted constructor parameter's own docblock after an attribute is ignored.** In the example below, the description and any `@var` type are lost, so hover can show the constructor's `@param` text instead, and inference the wrong type, not just a missing one. A docblock before the attribute works.

  ```php
  public function __construct(
      #[\SensitiveParameter]
      /** Own text. */
      public string $secret,
  ) {}
  ```

- **The browser (WebAssembly) build has no declaration hover.** It has no command line and reads no initialization options, and the on-by-default switch applies only to the native binaries.
