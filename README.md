# usagebench

Repository for creating and running benchmarks around the static analysis task of discovering usages of certain code units.

## Directory Structure

* `javagen`: A Joern-based Java usage extractor.

## Generating Java Usages

1. Build `javagen`:
   ```
   cd javagen
   sbt stage
   ```
2. Build Java usages from `dataset/java_repositories.csv`
   ```
   ./javagen/javagen dataset/java_repositories.csv dataset/java
   ```