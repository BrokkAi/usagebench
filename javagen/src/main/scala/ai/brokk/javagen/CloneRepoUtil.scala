package ai.brokk.javagen

import org.slf4j.LoggerFactory

import java.io.File
import java.nio.charset.MalformedInputException
import java.nio.file.{Files, Path}
import scala.sys.process.*
import scala.util.Try

object CloneRepoUtil {

  private val logger = LoggerFactory.getLogger(getClass)

  /** Runs a shell command in a specified directory, logging its output via SLF4J.
    *
    * @param cmd
    *   The command sequence (e.g., Seq("git", "fetch"))
    * @param cwd
    *   The working directory (java.io.File)
    * @return
    *   Success(()) if the command succeeds, Failure(exception) otherwise.
    */
  private def runShellCommand(cmd: Seq[String], cwd: File): Try[Unit] = Try {
    val err = new StringBuilder
    val out = new StringBuilder

    // A logger to capture both stdout and stderr
    val processLogger = ProcessLogger(
      stdoutLine => out.append(stdoutLine).append("\n"),
      stderrLine => err.append(stderrLine).append("\n")
    )

    logger.info(s"  [RUN] ${cmd.mkString(" ")} (in $cwd)")
    val exitCode = Process(cmd, cwd).!(processLogger)

    val stdout = out.toString().trim
    val stderr = err.toString().trim

    if (exitCode != 0) {
      // Log captured output on failure
      logger.error(s"  [ERROR] Command failed (exit code $exitCode): ${cmd.mkString(" ")}")
      if (stdout.nonEmpty) logger.warn(s"  [STDOUT] $stdout")
      if (stderr.nonEmpty) logger.error(s"  [STDERR] $stderr")
      throw new RuntimeException(s"Command failed with exit code $exitCode: ${cmd.mkString(" ")}")
    } else {
      // Log stdout on success for visibility
      if (stdout.nonEmpty) logger.info(s"  [OUT] ${stdout.replaceAll("\n", "\n  [OUT] ")}")
    }
  }

  /** Processes a single repository (one line from the CSV).
    *
    * @param line
    *   The CSV line "repoUrl,commitSha"
    * @param baseDir
    *   The base directory to clone repos into.
    * @return
    *   Success(()) if processing succeeds, Failure(exception) otherwise.
    */
  def processRepo(line: String, baseDir: Path): Try[Path] = Try {
    line.split(',') match {
      case Array(repoUrlStr, commitShaStr) =>
        val repoUrl   = repoUrlStr.trim
        val commitSha = commitShaStr.trim

        if (repoUrl.isEmpty || commitSha.isEmpty) {
          throw new IllegalArgumentException("Repo URL or SHA is empty.")
        }

        // Extract a simple repo name, e.g., "commons-io"
        val repoName = repoUrl.split('/').last.stripSuffix(".git")
        val repoPath = baseDir.resolve(repoName)

        logger.info(s"--- Processing $repoName ---")

        val gitOps: Try[Unit] = if (Files.isDirectory(repoPath)) {
          // Case 1: Repo exists. Fetch and checkout.
          logger.info(s"Repository exists at $repoPath. Fetching...")
          for {
            _ <- runShellCommand(Seq("git", "fetch", "--all"), repoPath.toFile)
            _ = logger.info(s"Checking out commit $commitSha...")
            _ <- runShellCommand(Seq("git", "checkout", commitSha), repoPath.toFile)
          } yield ()
        } else {
          // Case 2: Repo doesn't exist. Clone and checkout.
          logger.info(s"Cloning $repoUrl to $repoPath...")
          for {
            _ <- runShellCommand(Seq("git", "clone", repoUrl, repoPath.toString), baseDir.toFile)
            _ = logger.info(s"Checking out commit $commitSha...")
            // The checkout command must run *inside* the newly cloned repo
            _ <- runShellCommand(Seq("git", "checkout", commitSha), repoPath.toFile)
          } yield ()
        }

        // This will re-throw any exception caught during the git operations
        gitOps.get
        logger.info(s"Successfully processed $repoName at $commitSha.")
        repoPath
      case _ =>
        throw new RuntimeException(s"Skipping malformed line: $line")
    }
  }

}
