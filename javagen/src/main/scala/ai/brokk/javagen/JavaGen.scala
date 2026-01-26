package ai.brokk.javagen

import io.joern.javasrc2cpg.JavaSrc2Cpg.language
import io.joern.javasrc2cpg.passes.{AstCreationPass, OuterClassRefPass, TypeInferencePass}
import io.joern.javasrc2cpg.{JavaSrc2Cpg, Config as JavaConfig}
import io.joern.x2cpg.{SourceFiles, X2Cpg}
import io.joern.x2cpg.X2Cpg.withNewEmptyCpg
import io.joern.x2cpg.passes.frontend.{JavaConfigFileCreationPass, MetaDataPass, TypeNodePass}
import io.shiftleft.codepropertygraph.generated.Cpg
import org.slf4j.LoggerFactory
import upickle.default.*

import java.nio.file.{Files, Path, StandardOpenOption}
import scala.jdk.CollectionConverters.*
import scala.io.{Codec, Source}
import scala.util.{Failure, Success, Try, Using}

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
    val projectName = inputPath.getFileName
    val usagesFile  = config.outputDir.resolve(s"$projectName-usages.json")
    if (Files.exists(usagesFile)) {
      logger.info(s"$usagesFile already exists, skipping...")
    } else {
      val tempCpgPath = Files.createTempFile("brokk-usages-", ".bin")
      try {
        val javaConfig = JavaConfig()
          .withInputPath(inputPath.toAbsolutePath.toString)
          .withOutputPath(tempCpgPath.toAbsolutePath.toString)
          .withDefaultIgnoredFilesRegex(Nil)
          .withIgnoredFiles(Nil)
        createCpg(javaConfig) match {
          case Success(cpg) =>
            logger.info(s"Created AST for ${config.inputPath}")
            X2Cpg.applyDefaultOverlays(cpg)
            logger.info(s"Created CFG, type hierarchy, etc. Analyzing usages...")
            val usages = UsageAnalyzers.analyze(cpg)
            logger.info(s"Usage analysis complete, writing....")
            val serializedUsages = write(usages, indent = 3, sortKeys = true)
            Files.writeString(usagesFile, serializedUsages, StandardOpenOption.CREATE_NEW, StandardOpenOption.WRITE)
            logger.info(s"Usage analysis results written to $usagesFile")
          case Failure(exception) =>
            logger.error("Exception encountered while creating AST", exception)
        }
      } catch {
        case e: Exception => logger.error("Exception encountered while analyzing CPG", e)
      } finally {
        Files.deleteIfExists(tempCpgPath)
      }
    }
  }

  private def createCpg(config: JavaConfig): Try[Cpg] = {
    withNewEmptyCpg(config.outputPath, config: JavaConfig) { (cpg, config) =>
      new MetaDataPass(cpg, language, config.inputPath).createAndApply()
      val sourceFiles = SourceFiles.determine(
        config.inputPath,
        JavaSrc2Cpg.sourceFileExtensions,
        ignoredDefaultRegex = Option(config.defaultIgnoredFilesRegex),
        ignoredFilesRegex = Option(config.ignoredFilesRegex),
        ignoredFilesPath = Option(config.ignoredFiles)
      )
      val astCreationPass = new AstCreationPass(config, cpg, Some(sourceFiles))
      astCreationPass.createAndApply()
      astCreationPass.sourceParser.cleanupDelombokOutput()
      astCreationPass.clearJavaParserCaches()
      new OuterClassRefPass(cpg).createAndApply()
      JavaConfigFileCreationPass(cpg).createAndApply()
      if (!config.skipTypeInfPass) {
        TypeNodePass.withRegisteredTypes(astCreationPass.global.usedTypes.keys().asScala.toList, cpg).createAndApply()
        new TypeInferencePass(cpg).createAndApply()
      }
    }
  }

}
