<?php
#[Attribute] class Tag {}
class Box {
    #[Tag]
    /** @var list<string> Own items text. */
    public array $items = [];
    #[\Deprecated]
    /** @return list<string> Own names text. */
    public function names(): array { return []; }
    /** @var list<string> Control text. */
    #[Tag]
    public array $control = [];
}
$b = new Box();
echo $b->items;
$n = $b->names();
echo $b->control;
