package ai.brokk.javagen

import org.eclipse.jdt.core.dom.*
import org.slf4j.LoggerFactory

import java.nio.file.Path
import scala.collection.mutable
import scala.jdk.CollectionConverters.*

object UsageAnalyzers {

  private val logger = LoggerFactory.getLogger(getClass)

  private val TYPE     = "CLASS"
  private val FIELD    = "FIELD"
  private val FUNCTION = "FUNCTION"

  def analyze(sources: Seq[Path]): ProgramUsages = {
    val parser = ASTParser.newParser(AST.JLS_Latest)
    parser.setResolveBindings(true)
    parser.setBindingsRecovery(true)
    parser.setKind(ASTParser.K_COMPILATION_UNIT)

    val sourceDirs = sources.map(_.getParent.toAbsolutePath.toString).distinct.toArray
    parser.setEnvironment(null, sourceDirs, null, true)

    val sourceFiles = sources.map(_.toAbsolutePath.toString).toArray
    val collector   = new UsageCollector(sourceFiles.toSet)

    parser.createASTs(
      sourceFiles,
      null,
      Array.empty[String],
      collector,
      null
    )

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

      ast.accept(new ASTVisitor() {

        private def getFqn(binding: ITypeBinding): String =
          binding.getErasure.getQualifiedName.replace("$", ".")

        private def getMethodFqn(binding: IMethodBinding): String =
          val decl = binding.getMethodDeclaration
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
          val (declKey, declElement) = binding match {
            case b: IMethodBinding =>
              val d = b.getMethodDeclaration
              (d.getKey, d.getJavaElement)
            case b: IVariableBinding =>
              val d = b.getVariableDeclaration
              (d.getKey, d.getJavaElement)
            case b: ITypeBinding =>
              val d = b.getTypeDeclaration
              (d.getKey, d.getJavaElement)
            case _ =>
              (binding.getKey, binding.getJavaElement)
          }

          if (declElement == null) return

          // Filter: only record references to "Application Code" (files we are parsing)
          val path = declElement.getPath
          if (path == null || !inputFiles.contains(path.toOSString)) return

          val location = resolveLocation(node)
          collectedUsages.getOrElseUpdate(declKey, mutable.ListBuffer.empty) += location
        }

        private def resolveLocation(node: ASTNode): UsageLocation = {
          var current = node
          var found   = false
          var name    = "unknown"

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
          val line = ast.getLineNumber(node.getStartPosition)
          UsageLocation(name, line)
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
        override def visit(node: SimpleName): Boolean = {
          val b = node.resolveBinding()
          if (b != null) recordUsage(b, node)
          true
        }

        override def visit(node: MethodInvocation): Boolean = {
          val b = node.resolveMethodBinding()
          if (b != null) recordUsage(b, node)
          true
        }

        override def visit(node: ClassInstanceCreation): Boolean = {
          val b = node.resolveConstructorBinding()
          if (b != null) recordUsage(b, node)
          true
        }

        override def visit(node: FieldAccess): Boolean = {
          val b = node.resolveFieldBinding()
          if (b != null) recordUsage(b, node)
          true
        }
      })
    }
  }
}
