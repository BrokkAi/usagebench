package ai.brokk.javagen

import io.joern.x2cpg.Defines
import io.shiftleft.codepropertygraph.generated.{Cpg, Operators}
import io.shiftleft.semanticcpg.language.*
import io.shiftleft.codepropertygraph.generated.language.*
import io.shiftleft.codepropertygraph.generated.nodes.*
import org.slf4j.LoggerFactory

import scala.util.*

object UsageAnalyzers {

  private val logger = LoggerFactory.getLogger(getClass)

  private val CLASS    = "CLASS"
  private val FIELD    = "FIELD"
  private val FUNCTION = "FUNCTION"

  def analyze(cpg: Cpg): ProgramUsages = {
    val usages = cpg.typeDecl
      .whereNot(_.isLambda)
      .whereNot(_.isExternal(true))
      .whereNot(_.file.name(".*/test/.*"))
      .flatMap(analyzeTypeDecl(cpg, _))
      .l
    ProgramUsages(usages)
  }

  private def analyzeTypeDecl(cpg: Cpg, typeDecl: TypeDecl): List[CodeUnitUsages] = Try {
    // Handle type usages here
    val typeFullName = typeDecl.fullName
    val typeUsages: List[UsageLocation] = typeDecl.referencingType.evalTypeIn.flatMap {
      case x: MethodParameterIn =>
        val lineNo = x.lineNumber.orElse(x.method.lineNumber).getOrElse(-1)
        UsageLocation(x.method.fqName, lineNo) :: Nil
      case x: Local =>
        x.method
          .map(_.fqName)
          .flatMap { methodFullName =>
            x.referencingIdentifiers
              .where(i => i.inAssignment.source.typ.fullNameExact(typeFullName))
              .map { i =>
                val lineNo = i.lineNumber.orElse(i.method.lineNumber).getOrElse(-1)
                UsageLocation(methodFullName, lineNo)
              }
          }
          .l
      case x: Call if x.name == Defines.ConstructorMethodName || x.name == Defines.StaticInitMethodName =>
        val methodFullName = x.method.fqName
        val lineNo         = x.lineNumber.orElse(x.method.lineNumber).getOrElse(-1)
        UsageLocation(methodFullName, lineNo) :: Nil
      case _ => List.empty[UsageLocation]
    }.toList
    val typeUnitUsages = CodeUnitUsages(typeDecl.fqName, CLASS, typeUsages) :: Nil

    // Handle fields
    val fieldUnitUsages = typeDecl.member.flatMap { member =>
      analyzeField(cpg, member) match {
        case Success(result) => result :: Nil
        case Failure(e) =>
          logger.error(s"Unable to analyze usages for field ${member.fqName}", e)
          Nil
      }
    }.l

    // Handle methods
    val methodUnitUsages = typeDecl.method
      .whereNot(_.isLambda)
      .flatMap { method =>
        analyzeMethod(cpg, method) match {
          case Success(result) => result :: Nil
          case Failure(e) =>
            logger.error(s"Unable to analyze usages for method ${method.fullName}", e)
            Nil
        }
      }
      .l

    // Combine results
    typeUnitUsages ++ fieldUnitUsages ++ methodUnitUsages
  } match {
    case Success(result) =>
      // Filter things that should really be avoided
      def weirdThingFilter(fqName: String): Boolean = fqName.contains("<lambda>") ||
        fqName.contains("<unresolvedSignature>") || fqName.endsWith("[]")

      result
        .filterNot { case CodeUnitUsages(fullyQualifiedName, _, _) => weirdThingFilter(fullyQualifiedName) }
        .map { codeUnitUsages =>
          // TODO: These hits might ideally like to be transformed to parent class instead
          codeUnitUsages.copy(usages = codeUnitUsages.usages.filterNot(u => weirdThingFilter(u.fullyQualifiedName)))
        }
    case Failure(e) =>
      logger.error(s"Unable to analyze usages for type ${typeDecl.fullName}", e)
      Nil
  }

  private def analyzeField(cpg: Cpg, member: Member): Try[CodeUnitUsages] = Try {
    val usages = member._refIn
      .collect { case x: Call if x.name == Operators.fieldAccess => x }
      .map { fieldAccess =>
        val methodFullName = fieldAccess.method.fqName
        val lineNo         = fieldAccess.lineNumber.orElse(fieldAccess.method.lineNumber).getOrElse(-1)
        UsageLocation(methodFullName, lineNo)
      }
      .l
    CodeUnitUsages(member.fqName, FIELD, usages)
  }

  private def analyzeMethod(cpg: Cpg, method: Method): Try[CodeUnitUsages] = Try {
    val usages = method
      .callIn(NoResolve)
      .map { methodCall =>
        val methodFullName = methodCall.method.fqName
        val lineNo         = methodCall.lineNumber.orElse(methodCall.method.lineNumber).getOrElse(-1)
        UsageLocation(methodFullName, lineNo)
      }
      .l
    CodeUnitUsages(method.fqName, FUNCTION, usages)
  }

  extension (m: Method) {

    /** @return
      *   normalized name, i.e., <code>&ltinit&gt</code> becomes the defining type name.
      */
    def normalizedName: String = {
      m.name match {
        case Defines.ConstructorMethodName | Defines.StaticInitMethodName => m.typeDecl.map(_.name).getOrElse(m.name)
        case name                                                         => name
      }
    }

    /** @return
      *   normalized fqName as per Brokk's standards for Java.
      */
    def fqName: String = {
      m.typeDecl
        .map { definingTypeDecl =>
          val typeFullName = definingTypeDecl.fqName
          typeFullName + "." + m.normalizedName
        }
        .getOrElse(m.fullName)
    }

  }

  extension (t: TypeDecl) {

    /** @return
      *   normalized fqName as per Brokk's standards for Java.
      */
    def fqName: String = t.fullName.replace("$", ".")

  }

  extension (m: Member) {

    /** @return
      *   normalized fqName as per Brokk's standards for Java.
      */
    def fqName: String = m.typeDecl.fqName + "." + m.name

  }

}
