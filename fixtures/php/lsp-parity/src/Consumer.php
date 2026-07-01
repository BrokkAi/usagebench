<?php

namespace App;

use App\Service\EmailNotifier as Mailer;

$mailer = Mailer::create();
$mailer->notify("hello");
$mailer->record("logged");
$count = Mailer::$sent;
$value = $mailer->dynamicName;
