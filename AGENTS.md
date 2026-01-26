# Scala 3 Coding Style Guide

This guide outlines the specific conventions used in the `brokk-javagen` codebase, focusing on Scala 3 features and Joern-specific patterns.

## 1. Modern Scala Syntax (Scala 3)

The codebase utilizes the new "quiet" indentation-based syntax and other Scala 3 improvements.

*   **Significant Indentation:** Prefer indentation over braces for control flow.
    *   Use `then` for `if` statements.
    *   Omit braces for `class`, `object`, and `def` when possible.
*   **Wildcard Imports:** Use `*` instead of `_` for wildcard imports (e.g., `import upickle.default.*`).
*   **Explicit Imports:** When aliasing or importing multiple specific members, use the new `as` and `{}` syntax (e.g., `import io.joern.javasrc2cpg.{JavaSrc2Cpg, Config as JavaConfig}`).

## 2. Domain Modeling & Extensions

*   **Extensions:** Use `extension` blocks to add domain-specific logic to external library classes (e.g., Joern's `Method`, `TypeDecl`, and `Member` nodes). This keeps analysis logic clean and readable.
*   **Opaque Constants:** Define internal string constants in uppercase (e.g., `private val CLASS = "CLASS"`) within objects to act as simple enums for serialized output.

## 3. Control Flow & Functional Patterns

*   **Pattern Matching on Types:** Use Scala 3's simplified pattern matching for type casting in `flatMap` and `collect` operations.
*   **Try/Match for Error Recovery:** Wrap risky analysis blocks in `Try` and handle `Success`/`Failure` explicitly, logging errors instead of crashing the pipeline.
    ```scala
    analyzeMethod(cpg, method) match {
      case Success(result) => result :: Nil
      case Failure(e) =>
        logger.error(s"message", e)
        Nil
    }
    ```
*   **The `.l` Suffix:** When working with Joern's `overflowdb` traversals, explicitly use `.l` to convert traversals to lists when the pipeline transitions from lazy evaluation to eager processing.

## 4. Resource Management

*   **Using.resource:** Use `scala.util.Using` for safe handling of `Closeable` resources like `Source.fromFile`.
*   **Temporary Files:** Always use `try...finally` blocks to ensure temporary resources (like `tempCpgPath`) are deleted regardless of execution success.

## 5. Formatting & Naming

*   **Vertical Alignment:** Align assignment operators (`=`) and arrows (`=>`) in significant blocks (like `scopt` configurations or complex `match` cases) to improve readability.
*   **Fluent API Chaining:** When configuring builders, place the period at the start of the new line (e.g., `.withInputPath(...)`).
*   **Extension Method Naming:** Use descriptive names like `fqName` (Fully Qualified Name) for extension methods that normalize external library properties.

## 6. CLI & Configuration

*   **Scopt Integration:** Use `OParser.builder` for declarative CLI argument parsing.
*   **Validation:** Perform side effects (like directory creation) or complex validation directly within the `validate` block of the argument parser to fail fast.
*   **Path Handling:** Prefer `java.nio.file.Path` over `String` or `File` for filesystem paths. Use `.toAbsolutePath.normalize()` to ensure path consistency.