<?php

namespace Precision;

interface Notifier {
    public function send(string $message): void;
}
