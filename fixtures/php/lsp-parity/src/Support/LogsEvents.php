<?php

namespace App\Support;

trait LogsEvents
{
    public function record(string $message): string
    {
        return $message;
    }
}
