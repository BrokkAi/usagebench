package ai.brokk.javagen

import scopt.OParser

import java.nio.file.{Files, Path}

object Main {

  private val parser = {
    val builder = scopt.OParser.builder[Config]
    import builder.*

    OParser.sequence(
      programName("javagen"),
      help("help"),
      arg[Path]("input-path")
        .validate {
          case dir if Files.isDirectory(dir) => success
          case path if isCsvFile(path)       => success
          case path                          => failure(s"$path is neither a directory nor a CSV file")
        }
        .action {
          case (x, c) if isCsvFile(x) => c.copy(inputPath = x.toAbsolutePath.normalize(), inputIsCSV = true)
          case (x, c)                 => c.copy(inputPath = x.toAbsolutePath.normalize())
        }
        .text("Input directory of a Java project or CSV file of Git repositories ('git-address','commit-hash')"),
      arg[Path]("output-dir")
        .validate { dir =>
          if !Files.isDirectory(dir) then Files.createDirectories(dir)
          success
        }
        .action((x, c) => c.copy(outputDir = x.toAbsolutePath.normalize()))
        .text("Output directory")
    )
  }

  def main(args: Array[String]): Unit = {
    OParser.parse(parser, args, Config()).foreach(config => JavaGen(config).run())
  }

  def isCsvFile(path: Path): Boolean = {
    Files.isRegularFile(path) && path.getFileName.toString.endsWith(".csv")
  }

}

case class Config(
                   inputPath: Path = Path.of("."),
                   outputDir: Path = Path.of("./javagen_output").toAbsolutePath,
                   inputIsCSV: Boolean = false
)
