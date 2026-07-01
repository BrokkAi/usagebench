<?php

namespace App\Contracts;

interface Notifier
{
    public function notify(string $message): void;
}
