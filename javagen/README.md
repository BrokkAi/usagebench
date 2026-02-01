# JavaGen

An Eclipse JDT-based tool to extract usages from Java source-code applications.

## Requirements

* sbt
* git
* JDK 17+

## Getting Started

```
sbt stage
./javagen --help
Usage: javagen [options] input-path output-dir

  --help
  input-path  Input directory of a Java project or CSV file of Git repositories ('git-address','commit-hash')
  output-dir  Output directory
```
