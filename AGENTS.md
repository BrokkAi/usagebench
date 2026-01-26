# Coding Style Guide

This project uses Scala 3 and a few codebase-specific conventions. The guide below focuses on the conventions you should follow that are specific, uncommon, or use new Scala 3 features. Do not repeat common best practices.

---

## Scala 3 / language & formatting

- Prefer Scala 3 significant indentation style (no braces) for control structures and method bodies:
  - Use `if ... then ...` and `try ... finally` indentation forms.
  - Method definitions and blocks normally omit braces when using indentation.
  - Example:
    ```
    def run(): Unit =
      if config.inputIsCSV
      then runCsv(config.inputPath)
      else runDir(config.inputPath)
    ```

- Use the terse single-line method call style for simple delegation (no parentheses when no arguments or side effects are implied).

- Use `import x.*` (Scala 3 wildcard import) for DSL-like language imports (e.g., `import builder.*`).

- Use `import A.{B, C as D}` aliasing when a name clashes or clarity is needed:
  - Example: `import io.joern.javasrc2cpg.{JavaSrc2Cpg, Config as JavaConfig}`

---

## Extension methods

- Prefer `extension` to add utility methods on external classes (Scala 3 feature).
- Keep these grouped and documented with ScalaDoc-style comments where the method is nontrivial.
- Example pattern:
  ```
  extension (m: Method)
    /** normalized name: <init> becomes the defining type name */
    def normalizedName: String = ...
  ```

- Use extension methods to provide normalized, codebase-specific projections (e.g., `fqName`, `normalizedName`) rather than adding logic inline.

---

## Imports & DSLs

- When using domain-specific DSLs (e.g., codepropertygraph / semanticcpg), import the DSL wildcard and generated language packages for concise queries:
  - `import io.shiftleft.semanticcpg.language.*`
  - `import io.shiftleft.codepropertygraph.generated.language.*`

- Use the DSL’s collection-to-List helper `.l` when converting query results to Scala collections explicitly.

---

## Error handling and resilience

- Use `scala.util.Try` liberally for per-item analysis and to ensure a single failure doesn't abort processing of other items.
  - Return `Try[T]` from helper analyzers and pattern-match on `Success` / `Failure` in callers.
  - Log failures with context and continue processing.
  - Example:
    ```
    analyzeField(...) match
      case Success(result) => ...
      case Failure(e) =>
        logger.error(s"Unable to analyze usages for field ${member.fqName}", e)
    ```

- Use pattern matching on `Try` and `Option` results rather than nested `if` checks where appropriate.

- For resource safety use `Using.resource` (scala.util.Using) to handle IO resources:
  - Example:
    ```
    Using.resource(Source.fromFile(csvPath.toFile)(Codec.UTF8)) { source =>
      source.getLines().foreach { line => ... }
    }
    ```

---

## Logging

- Create a logger per class/object:
  - `private val logger = LoggerFactory.getLogger(getClass)`
- Include exception objects in error logs (pass the exception as the last parameter to `logger.error`).
- Use informative messages including the failing entity.

---

## IO & Path handling

- Use java.nio Path APIs and `Path.of` defaults via `case class` defaults for configuration:
  - Store normalized / absolute paths as soon as possible:
    ```
    action((x, c) => c.copy(outputDir = x.toAbsolutePath.normalize()))
    ```

- Use `Files` convenience methods (e.g., `Files.exists`, `Files.createTempFile`, `Files.writeString`) and `StandardOpenOption` where appropriate.

---

## scopt CLI usage

- Use scopt builder and import builder.*; prefer functional `validate` and `action` combinators.
- Perform validation inside the `arg` call, including side-effecting directory creation in `validate` when convenient:
  ```
  .validate { dir =>
    if !Files.isDirectory(dir) then Files.createDirectories(dir)
    success
  }
  ```

- Use `OParser.sequence` for parser composition.

---

## Naming & constants

- Use UPPER_CASE for constant values used as discriminators:
  - Example: `private val CLASS = "CLASS"`
- Use camelCase for methods/values; PascalCase for types and case classes.
- When normalizing fully-qualified names, prefer replacing `$` with `.` for Java inner classes:
  - Provide explicit `fqName` extension methods for `TypeDecl`, `Method`, `Member`.

---

## Pattern matching & collection processing

- Use pattern matching in `flatMap`/`collect` to handle specific node types from the CPG/DSL.
- Favor functional combinators (`map`, `flatMap`, `collect`, `whereNot`) over imperative loops where the DSL supports it.
- Where you need to mix DSL queries and Scala collections, convert at boundaries with `.l`.

---

## Small domain-specific idioms

- For line number resolution: prefer `orElse(...).getOrElse(-1)` to select a sensible fallback:
  ```
  val lineNo = x.lineNumber.orElse(x.method.lineNumber).getOrElse(-1)
  ```

- Use `NoResolve` or other DSL options explicitly when calling DSL query methods that accept resolution options:
  ```
  method.callIn(NoResolve)
  ```

- When matching specific CPG operator names, compare to constants from the generated APIs (e.g., `Operators.fieldAccess`, `Defines.ConstructorMethodName`).

---

## Documentation & comments

- Use concise ScalaDoc for public/nontrivial extension methods or transformations describing normalized semantics (e.g., what "normalized" means).
- Keep inline comments to clarify why something unusual is done (not what is being done).

---

Follow these conventions to keep code consistent with the codebase's use of Scala 3 features, DSL usage, and the resilience pattern for static-analysis pipelines.