package ai.brokk.javagen

import java.nio.file.*
import java.nio.file.attribute.BasicFileAttributes
import scala.collection.mutable
import scala.jdk.CollectionConverters.*

class InlineTestProject private (val root: Path, val javaSources: Seq[Path]) extends AutoCloseable:

  override def close(): Unit =
    if Files.exists(root) then
      Files.walkFileTree(
        root,
        new SimpleFileVisitor[Path]:
          override def visitFile(file: Path, attrs: BasicFileAttributes): FileVisitResult =
            Files.delete(file)
            FileVisitResult.CONTINUE

          override def postVisitDirectory(dir: Path, exc: java.io.IOException): FileVisitResult =
            Files.delete(dir)
            FileVisitResult.CONTINUE
      )

object InlineTestProject:
  def builder(): Builder = new Builder()

  class Builder:
    private val files = mutable.ListBuffer.empty[(String, String)]

    def addFile(relPath: String, content: String): Builder =
      files.addOne((relPath, content))
      this

    def build(): InlineTestProject =
      val tempDir = Files.createTempDirectory("javagen-test-")
      val writtenPaths = files.map { (relPath, content) =>
        val target = tempDir.resolve(relPath)
        Files.createDirectories(target.getParent)
        Files.writeString(target, content)
        target.toAbsolutePath
      }.toSeq

      new InlineTestProject(tempDir, writtenPaths.filter(_.toString.endsWith(".java")))
