package ai.brokk.javagen

import org.slf4j.LoggerFactory
import upickle.default.*

import java.nio.file.{Files, Path, StandardOpenOption}
import scala.io.{Codec, Source}
import scala.jdk.CollectionConverters.*
import scala.util.{Failure, Success, Using}

class JavaGen(config: Config) {

  private val logger = LoggerFactory.getLogger(getClass)

  def run(): Unit = {
    if config.inputIsCSV
    then runCsv(config.inputPath)
    else runDir(config.inputPath)
  }

  private def runCsv(csvPath: Path): Unit = {
    logger.info(s"Reading repositories from: $csvPath")
    Using.resource(Source.fromFile(csvPath.toFile)(Codec.UTF8)) { source =>
      source.getLines().foreach { line =>
        if (line.trim.nonEmpty) { // Skip empty lines
          // Process each line and recover from failures to continue with the next
          CloneRepoUtil.processRepo(line, config.outputDir) match {
            case Success(path) =>
              runDir(path)
            case Failure(e) =>
              logger.error(s"Failed to process line '$line': ${e.getMessage}", e)
          }
        }
      }
    }
  }

  private def runDir(inputPath: Path): Unit = {
    val projectName = inputPath.getFileName.toString
    val usagesFile  = config.outputDir.resolve(s"$projectName-usages.json")

    if Files.exists(usagesFile) then
      logger.info(s"$usagesFile already exists, skipping...")
    else
      try
        val sources = findJavaSources(inputPath)
        if sources.isEmpty then
          logger.warn(s"No Java source files found in $inputPath")
        else
          logger.info(s"Analyzing usages for ${sources.size} source files in $inputPath...")
          val usages = UsageAnalyzers.analyze(sources)

          logger.info(s"Usage analysis complete, writing to $usagesFile...")
          val serializedUsages = write(usages, indent = 3, sortKeys = true)
          Files.writeString(usagesFile, serializedUsages, StandardOpenOption.CREATE, StandardOpenOption.TRUNCATE_EXISTING)
          logger.info(s"Usage analysis results successfully written.")
      catch
        case e: Exception =>
          logger.error(s"Exception encountered while analyzing $inputPath", e)
  }

  private def findJavaSources(root: Path): Seq[Path] = {
    val stream = Files.walk(root)
    try {
      stream
        .filter(p => Files.isRegularFile(p) && p.toString.endsWith(".java"))
        .toList
        .asScala
        .toSeq
    } finally {
      stream.close()
    }
  }

}
