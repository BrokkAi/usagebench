<?php

namespace Example;

final class Defaults
{
    public const PREFIX = 'job';
}

final class Repository
{
    public string $last = '';

    public function save(string $value): string
    {
        $this->last = trim($value);
        return $this->last;
    }
}

final class Service
{
    public function __construct(private Repository $repository)
    {
    }

    public function execute(string $name): string
    {
        $stored = $this->repository->save($name);
        return Defaults::PREFIX . ':' . $stored;
    }
}
