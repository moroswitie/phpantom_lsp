<?php
class Vault {
    public function __construct(
        #[\SensitiveParameter]
        /** @var non-empty-string Own text. */
        public string $secret,
        /** @var positive-int Before text. */
        #[\SensitiveParameter]
        public int $pin,
    ) {}
}
$v = new Vault('x', 1);
echo $v->secret;
echo $v->pin;
