<?php

namespace Example;

final class Consumer
{
    public static function run(): string
    {
        $repository = new Repository();
        $service = new Service($repository);
        $result = $service->execute(' Ada ');
        return Defaults::PREFIX . $result . $repository->last;
    }
}
