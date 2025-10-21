package ai.brokk.javagen

import io.shiftleft.codepropertygraph.generated.Cpg

object UsageAnalyzers {

  def analyze(cpg: Cpg): ProgramUsages = {
    ProgramUsages(Nil)
  }
  
}
