package ai.brokk

import upickle.default.{ReadWriter, macroRW}

package object javagen {

  case class ProgramUsages(codeUnits: List[CodeUnitUsages])
  object ProgramUsages {
    given ReadWriter[ProgramUsages] = macroRW
  }

  case class CodeUnitUsages(fullyQualifiedName: String, `type`: String, usages: List[UsageLocation])
  object CodeUnitUsages {
    given ReadWriter[CodeUnitUsages] = macroRW
  }

  case class UsageLocation(
    fullyQualifiedName: String,
    lineNumber: Int,
    snippet: String,
    filePath: String,
    syntaxStyle: String
  )
  object UsageLocation {
    given ReadWriter[UsageLocation] = macroRW
  }

}
