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

// ── Hover: Modifiers ────────────────────────────────────────────────────────

class HoverModifiersDemo
{
    public function demo(Scaffolding\ScaffoldingHoverRestrictions $value): string
    {
        // Try: hover on ScaffoldingHoverRestrictions above → final readonly class.
        // Try: hover on ScaffoldingHoverRestrictions below → constructor in the
        // same final readonly class wrapper.
        $created = new Scaffolding\ScaffoldingHoverRestrictions('created');

        // Try: hover on label → public string $label in a readonly class.
        // Try: hover on explicit → public readonly string $explicit.
        // Try: hover on sealed → final public function sealed(): string.
        return $value->label . $created->explicit . $value->sealed();
    }

    public function ordinary(Scaffolding\ScaffoldingHoverProperty $value): string
    {
        // Try: hover on label → public readonly string $label.
        // Try: hover on promoted → public readonly int $promoted.
        return $value->label . $value->promoted;
    }

    public function contract(Scaffolding\ScaffoldingHoverContract $value): string
    {
        // Try: hover on describe → abstract public function describe(): string.
        return $value->describe();
    }
}

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

// ── Hover: promoted property descriptions ───────────────────────────────────

class HoverPromotedDescriptionsDemo
{
    /**
     * @param int $fromParam Param description.
     * @param int $both Param description for both.
     */
    public function __construct(
        /** Own summary. */
        public int $ownSummary,
        /** @var int Own var description. */
        public int $ownVar,
        public int $fromParam,
        /** Own description for both. */
        public int $both,
    ) {}

    public function demo(): void
    {
        // Hover on `ownSummary` → "Own summary." (its own docblock)
        echo $this->ownSummary;

        // Hover on `ownVar` → "Own var description." (its own @var)
        echo $this->ownVar;

        // Hover on `fromParam` → "Param description." (the constructor's @param)
        echo $this->fromParam;

        // Hover on `both` → "Own description for both." (its own docblock wins over @param)
        echo $this->both;
    }
}

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
