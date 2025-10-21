package ai.brokk.javagen

import io.joern.javasrc2cpg.{JavaSrc2Cpg, Config as JavaConfig}

import scala.util.{Failure, Success}

class JavaGen(config: Config) {

  def run(): Unit = {
    val javaConfig = JavaConfig().withInputPath(config.inputDir.toAbsolutePath.toString)

    JavaSrc2Cpg().createCpg(javaConfig) match {
      case Success(cpg) =>
        println("[DONE]")
      case Failure(exception) =>
        println("[FAILED]")
        println(exception)
    }

  }

}
