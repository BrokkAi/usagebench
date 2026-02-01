name                     := "javagen"
ThisBuild / organization := "ai.brokk"
ThisBuild / scalaVersion := "3.5.2"

libraryDependencies ++= Seq(
  "com.github.scopt"        %% "scopt"               % Versions.scopt,
  "org.apache.logging.log4j" % "log4j-slf4j2-impl"   % Versions.log4j % Optional,
  "com.lihaoyi"             %% "ujson"               % Versions.ujson,
  "com.lihaoyi"             %% "upickle"             % "4.0.2",
  "org.eclipse.jdt"          % "org.eclipse.jdt.core" % Versions.jdt,
  "org.scalatest"           %% "scalatest"           % Versions.scalatest % Test
)

assembly / assemblyMergeStrategy := {
  case "log4j2.xml"                                             => MergeStrategy.first
  case "module-info.class"                                      => MergeStrategy.first
  case "META-INF/versions/9/module-info.class"                  => MergeStrategy.first
  case "io/github/retronym/java9rtexport/Export.class"          => MergeStrategy.first
  case PathList("scala", "collection", "internal", "pprint", _) => MergeStrategy.first
  case x =>
    val oldStrategy = (ThisBuild / assemblyMergeStrategy).value
    oldStrategy(x)
}

ThisBuild / Compile / scalacOptions ++= Seq("-feature", "-deprecation", "-language:implicitConversions")

enablePlugins(JavaAppPackaging)

ThisBuild / licenses := List("Apache-2.0" -> url("http://www.apache.org/licenses/LICENSE-2.0"))

Global / onChangedBuildSource := ReloadOnSourceChanges

ThisBuild / resolvers ++= Seq(
  Resolver.mavenLocal,
  "Sonatype OSS" at "https://oss.sonatype.org/content/repositories/public",
  "Atlassian" at "https://packages.atlassian.com/mvn/maven-atlassian-external",
  "Gradle Releases" at "https://repo.gradle.org/gradle/libs-releases/"
)

Compile / doc / sources                := Seq.empty
Compile / packageDoc / publishArtifact := false
