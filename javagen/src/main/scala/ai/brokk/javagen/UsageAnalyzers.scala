package ai.brokk.javagen

import org.eclipse.jdt.core.dom.*
import org.slf4j.LoggerFactory

import java.nio.file.{Files, Path}
import scala.collection.mutable
import scala.jdk.CollectionConverters.*

object UsageAnalyzers {

  private val logger = LoggerFactory.getLogger(getClass)

  private val TYPE     = "CLASS"
  private val FIELD    = "FIELD"
  private val FUNCTION = "FUNCTION"

  def analyze(sources: Seq[Path]): ProgramUsages = {
    val parser = ASTParser.newParser(AST.getJLSLatest)
    parser.setResolveBindings(true)
    parser.setBindingsRecovery(true)
    parser.setKind(ASTParser.K_COMPILATION_UNIT)

    val sourceDirs = sources.map(_.getParent.toAbsolutePath.toString).distinct.toArray
    parser.setEnvironment(null, sourceDirs, null, true)

    val sourceFiles = sources.map(_.toAbsolutePath.toString).toArray
    val collector   = new UsageCollector(sources.map(_.toAbsolutePath.toString).toSet)

    parser.createASTs(sourceFiles, null, Array.empty[String], collector, null)

    ProgramUsages(collector.result())
  }

  private class UsageCollector(inputFiles: Set[String]) extends FileASTRequestor {

    private val definitions = mutable.Map.empty[String, CodeUnitUsages]
    private val references  = mutable.ListBuffer.empty[(IBinding, UsageLocation)]

    def result(): List[CodeUnitUsages] = finalResult()

    private val bindingKeyToCodeUnit = mutable.Map.empty[String, CodeUnitUsages]
    private val collectedUsages      = mutable.Map.empty[String, mutable.ListBuffer[UsageLocation]]

    def finalResult(): List[CodeUnitUsages] = {
      bindingKeyToCodeUnit.toList.map { case (key, unit) =>
        val usages = collectedUsages.getOrElse(key, Nil).toList
        unit.copy(usages = usages)
      }
    }

    override def acceptAST(sourceFilePath: String, ast: CompilationUnit): Unit = {
      val isTest = sourceFilePath.contains("/test/") || sourceFilePath.contains("\\test\\")

      val fileLines = try {
        Files.readAllLines(Path.of(sourceFilePath)).asScala.toIndexedSeq
      } catch {
        case e: Exception =>
          logger.warn(s"Could not read source file for snippets: $sourceFilePath", e)
          IndexedSeq.empty[String]
      }

      ast.accept(new ASTVisitor() {

        private def getFqn(binding: ITypeBinding): String =
          binding.getErasure.getQualifiedName.replace("$", ".")

        private def getMethodFqn(binding: IMethodBinding): String =
          val decl    = binding.getMethodDeclaration
          val typeFqn = getFqn(decl.getDeclaringClass)
          s"$typeFqn.${if (decl.isConstructor) decl.getDeclaringClass.getName else decl.getName}"

        private def getVariableFqn(binding: IVariableBinding): String =
          val parent = if (binding.getDeclaringClass != null) getFqn(binding.getDeclaringClass) else "unknown"
          s"$parent.${binding.getName}"

        private def recordDef(key: String, fqn: String, kind: String): Unit = {
          if (!isTest) {
            bindingKeyToCodeUnit.getOrElseUpdate(key, CodeUnitUsages(fqn, kind, Nil))
          }
        }

        private def recordUsage(binding: IBinding, node: ASTNode): Unit = {
          if (binding == null) return

          // Normalize binding to its declaration to ensure keys match
          val declKey = binding match {
            case b: IMethodBinding   => b.getMethodDeclaration.getKey
            case b: IVariableBinding => b.getVariableDeclaration.getKey
            case b: ITypeBinding     => b.getTypeDeclaration.getKey
            case _                   => binding.getKey
          }

          // Only record usages for definitions we've seen (i.e., in the input files)
          if (!bindingKeyToCodeUnit.contains(declKey)) return

          val location = resolveLocation(node)
          collectedUsages.getOrElseUpdate(declKey, mutable.ListBuffer.empty) += location
        }

        private def resolveLocation(node: ASTNode): UsageLocation = {
          val absolutePath = Path.of(sourceFilePath).toAbsolutePath.toString
          var current      = node
          var found        = false
          var name         = "unknown"

          while (current != null && !found) {
            current match {
              case md: MethodDeclaration =>
                val b = md.resolveBinding()
                if (b != null && b.getDeclaringClass != null && !b.getDeclaringClass.isAnonymous) {
                  name = getMethodFqn(b)
                  found = true
                }
              case td: TypeDeclaration =>
                val b = td.resolveBinding()
                if (b != null && !b.isAnonymous) {
                  name = getFqn(b)
                  found = true
                }
              case _ =>
            }
            current = current.getParent
          }

          // If no method or type was found (e.g. in imports), attribute to the first top-level type in the file
          if (!found) {
            ast.types().asScala.headOption.collect { case td: TypeDeclaration =>
              val b = td.resolveBinding()
              if (b != null) {
                name = getFqn(b)
                found = true
              }
            }
          }

          val line    = ast.getLineNumber(node.getStartPosition)
          val snippet = captureSnippet(line)
          UsageLocation(
            fullyQualifiedName = name,
            lineNumber = line,
            snippet = snippet,
            filePath = absolutePath,
            syntaxStyle = "text/java"
          )
        }

        private def captureSnippet(line: Int): String = {
          if (fileLines.isEmpty || line <= 0) return ""
          val zeroBasedLine = line - 1
          val start = Math.max(0, zeroBasedLine - 3)
          val end   = Math.min(fileLines.size - 1, zeroBasedLine + 3)
          fileLines.slice(start, end + 1).mkString("\n")
        }

        override def visit(node: TypeDeclaration): Boolean = {
          val b = node.resolveBinding()
          if (b != null && !b.isAnonymous) recordDef(b.getKey, getFqn(b), TYPE)
          true
        }

        override def visit(node: MethodDeclaration): Boolean = {
          val b = node.resolveBinding()
          if (b != null && b.getDeclaringClass != null && !b.getDeclaringClass.isAnonymous)
            recordDef(b.getKey, getMethodFqn(b), FUNCTION)
          true
        }

        override def visit(node: VariableDeclarationFragment): Boolean = {
          val b = node.resolveBinding()
          if (b != null && b.isField) recordDef(b.getKey, getVariableFqn(b), FIELD)
          true
        }


        // References
        // We primarily use SimpleName to catch references to methods, fields, and types.
        // JDT's isDeclaration check prevents counting the definition site as a usage.
        override def visit(node: SimpleName): Boolean = {
          val b = node.resolveBinding()
          if (b != null && !node.isDeclaration) {
            // To avoid double-counting method calls (once as MethodInvocation and once as SimpleName),
            // we skip SimpleNames that are the name part of a method call or declaration.
            val parent = node.getParent
            val isIgnored = parent match {
              case mi: MethodInvocation if mi.getName == node  => true
              case md: MethodDeclaration if md.getName == node => true
              case _                                           => false
            }
            // Also check if any ancestor is an ImportDeclaration
            val isInImport = {
              var p: ASTNode = node.getParent
              var found = false
              while (p != null && !found) {
                if (p.isInstanceOf[ImportDeclaration]) found = true
                p = p.getParent
              }
              found
            }
            if (!isIgnored && !isInImport) recordUsage(b, node)
          }
          true
        }

        override def visit(node: MethodInvocation): Boolean = {
          val b = node.resolveMethodBinding()
          if (b != null) recordUsage(b, node)
          true
        }

        // Constructor calls often map to the Type name in SimpleName, 
        // but we want to ensure the MethodBinding (the constructor) is also recorded.
        override def visit(node: ClassInstanceCreation): Boolean = {
          val b = node.resolveConstructorBinding()
          if (b != null) recordUsage(b, node)
          true
        }
      })
    }
  }
}
