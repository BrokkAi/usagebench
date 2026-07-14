<?php

namespace Precision;

use function Precision\format;

final class EmailNotifier implements Notifier {
    public function send(string $message): void {}
}

function notify(Notifier $notifier): void {
    $notifier->send(format("hello"));
    Labels::create();
}
