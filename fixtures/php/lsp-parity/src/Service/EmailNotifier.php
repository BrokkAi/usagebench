<?php

namespace App\Service;

use App\Contracts\Notifier;
use App\Support\LogsEvents;

class EmailNotifier implements Notifier
{
    use LogsEvents;

    public static int $sent = 0;

    public function notify(string $message): void
    {
        self::$sent++;
        $this->record($message);
    }

    public static function create(): self
    {
        return new self();
    }

    public function __get(string $name): string
    {
        return $name;
    }
}
