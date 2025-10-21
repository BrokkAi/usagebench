package ai.brokk.javagen

import scopt.OParser

import java.nio.file.Path

object Main {

  private val parser = {
    val builder = scopt.OParser.builder[Config]
    import builder.*

    OParser.sequence(
      programName("javagen"),
      help("help"),
      arg[String]("input-dir")
        .action((x, c) => c.copy(inputDir = Path.of(x)))
        .text("Input directory")
    )
  }

  def main(args: Array[String]): Unit = {
    OParser.parse(parser, args, Config()).foreach(config => JavaGen(config).run())
  }

}

case class Config(inputDir: Path = Path.of("."), outputDir: Path = Path.of("."))
