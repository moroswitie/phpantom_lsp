<?php

/**
 * PHP Showcase — Hover
 *
 * What hovering a symbol shows, including where the symbol came from.
 *
 * One of the demo files listed in README.md. Supporting fixtures live in
 * scaffolding/scaffolding.php (namespace Demo\Scaffolding), and the runtime
 * assertions that verify the type claims in the comments below live in
 * scaffolding/assertions.php.
 */

namespace Demo;

use Demo\Scaffolding;

// ── Hover: Origin Indicators ────────────────────────────────────────────────

class HoverOriginsDemo extends Scaffolding\Model implements Scaffolding\Renderable
{
    public function demo(): void
    {
        // Hover on `format` → "◆ implements Scaffolding\Renderable"
        $this->format('earth');

        // Hover on `toArray` → "↑ overrides Scaffolding\Model"
        $this->toArray();

        // Hover on `getName` → no indicator (inherited, not overridden)
        $this->getName();
    }

    // Implements Scaffolding\Renderable (Scaffolding\Model has no format method)
    public function format(string $template): string { return ''; }

    // Overrides the abstract toArray() from Scaffolding\Model
    public function toArray(): array { return []; }
}

// ── Hover: Declarations (opt-in) ────────────────────────────────────────────

// Start the server with --hover-on-declarations (or pass
// "initializationOptions": {"hover": {"declarations": true}}) for this block.
// Without it, the hovers below answer nothing, as everywhere else.
//
// Try: hover on the `class` keyword below, on `DeclarationHoverDemo`, on
// `MAX_GREETINGS`, on `greeting`, or on `DECLARATION_GREETING` — each shows
// the same hover as hovering a use of it, instead of no hover at all.
//
// Try: hover on `$visits` in the constructor → the property it promotes
// (`private int $visits`), not a variable; hover on `nickname` in the
// `@property` tag → the same virtual `string` property `$demo->nickname`
// shows.
/**
 * @property string $nickname
 */
class DeclarationHoverDemo
{
    public const MAX_GREETINGS = 3;

    public function __construct(private int $visits = 0) {}

    public function greeting(): string { return 'hello'; }

    public function visits(): int { return $this->visits; }

    public function __get(string $name): string { return 'Ada'; }
}

const DECLARATION_GREETING = 'hello';

// ── Hover: Real Property Declarations ───────────────────────────────────────

/** @property list<string> $items */
class HoverRealPropertyDemo
{
    /** The stock kept by this class. */
    private array $items = ['paper'];

    public function demo(): array
    {
        // Try: hover on `items` → private array $items and its own description.
        return $this->items;
    }
}
