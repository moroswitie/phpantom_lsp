<?php

/** @property string $name Parent tag text. */
class TagParent { public function __get(string $n): mixed { return null; } }
/** @property non-empty-string $name */
class TagChildNoText extends TagParent {}
/** @property non-empty-string $name Child tag text. */
class TagChildText extends TagParent {}

class MP {
    /**
     * Parent summary.
     *
     * @param positive-int $id Parent param text.
     * @return non-empty-string Parent return text.
     */
    public function find(int $id): string { return 'x'; }

    /** @var non-empty-string Parent label text. */
    public string $label = 'x';
}
class MExplicitNoText extends MP {
    /** @param int $id */
    public function find(int $id): string { $__e = $id; return ''; }
}
class MNoDoc extends MP {
    public function find(int $id): string { $__n = $id; return 'x'; }
    public string $label = 'x';
}
class MSummaryOnly extends MP {
    /** Child summary only. */
    public function find(int $id): string { $__s = $id; return 'x'; }
}
class MInlineInherit extends MP {
    /** {@inheritDoc} Child extra text. */
    public function find(int $id): string { $__i = $id; return 'x'; }
}
class MTagInherit extends MP {
    /** @inheritDoc */
    public function find(int $id): string { $__t = $id; return 'x'; }
}

/** @property list<string> $items Tag text for items. */
class OwnRealDoc {
    /** @var array Real property text. */
    public array $items = [];
}

function probe(TagChildNoText $a, TagChildText $b, MExplicitNoText $e, MNoDoc $n, MSummaryOnly $s, MInlineInherit $i, MTagInherit $t, OwnRealDoc $o): void {
    $a->name;
    $b->name;
    $e->find(1);
    $n->find(1);
    $s->find(1);
    $i->find(1);
    $t->find(1);
    $n->label;
    $o->items;
    $__ret_e = $e->find(1);
    $__ret_n = $n->find(1);
    $__items = $o->items;
}
