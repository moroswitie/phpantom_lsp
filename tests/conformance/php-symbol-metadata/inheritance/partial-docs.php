<?php
class P {
    /**
     * @param positive-int $id
     * @param non-empty-string $name
     * @return non-empty-string
     */
    public function find(int $id, string $name): string { return 'x'; }
    /** @var non-empty-string */
    public string $label = 'x';
}
class ExplicitEqual extends P {
    /**
     * @param int $id
     * @return string
     */
    public function find(int $id, string $name): string { $__d1 = $id; $__d2 = $name; return ''; }
    /** @var string */
    public string $label = '';
}
class NoDoc extends P {
    public function find(int $id, string $name): string { $__d3 = $id; $__d4 = $name; return 'x'; }
    public string $label = 'x';
}
class DescriptionOnly extends P {
    /** Only a summary, no tags. */
    public function find(int $id, string $name): string { $__d5 = $id; $__d6 = $name; return 'x'; }
}
class Renamed extends P {
    public function find(int $key, string $title): string { $__d7 = $key; $__d8 = $title; return 'x'; }
}
class Grand extends NoDoc {
    public function find(int $id, string $name): string { $__d9 = $id; return 'x'; }
}
$__d10 = (new ExplicitEqual())->find(1, 'a');
$__d11 = (new NoDoc())->find(1, 'a');
$__d12 = (new ExplicitEqual())->label;
$__d13 = (new NoDoc())->label;
$__d14 = (new Grand())->find(1, 'a');
interface I1 { /** @return non-empty-string */ public function name(): string; }
interface I2 { /** @return lowercase-string */ public function name(): string; }
class Impl1 implements I1 { public function name(): string { return 'x'; } }
class Impl12 implements I1, I2 { public function name(): string { return 'x'; } }
class Impl21 implements I2, I1 { public function name(): string { return 'x'; } }
$__d15 = (new Impl1())->name();
$__d16 = (new Impl12())->name();
$__d17 = (new Impl21())->name();
trait T { /** @return non-empty-string */ public function t(): string { return 'x'; } }
class UsesT { use T; }
class OverridesT { use T; public function t(): string { return 'x'; } }
$__d18 = (new UsesT())->t();
$__d19 = (new OverridesT())->t();
