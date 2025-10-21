package ai.brokk

import upickle.default.*

package object javagen {
  
  case class ProgramUsages(codeUnits: List[CodeUnitUsages]) derives ReadWriter
  
  case class CodeUnitUsages(fullyQualifiedName: String, `type`: String, usages: List[UsageLocation]) derives ReadWriter
  
  case class UsageLocation(fullyQualifiedName: String, lineNumber: Int) derives ReadWriter
  
}
