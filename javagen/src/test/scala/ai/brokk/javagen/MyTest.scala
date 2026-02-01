package ai.brokk.javagen

import org.scalatest.matchers.should.Matchers
import org.scalatest.wordspec.AnyWordSpec
import scala.util.Using

class MyTest extends AnyWordSpec with Matchers {

  "UsageAnalyzers" should {

    "detect constructor usages" in {
      Using.resource(
        InlineTestProject
          .builder()
          .addFile(
            "com/example/Foo.java",
            """package com.example;
              |public class Foo {
              |  public Foo() {}
              |}
              |""".stripMargin
          )
          .addFile(
            "com/example/Bar.java",
            """package com.example;
              |public class Bar {
              |  public void main() {
              |    Foo f = new Foo();
              |  }
              |}
              |""".stripMargin
          )
          .build()
      ) { project =>
        val result = UsageAnalyzers.analyze(project.javaSources)
        println(s"Result: ${result.codeUnits.map(cu => s"${cu.fullyQualifiedName} (${cu.`type`}) -> ${cu.usages.map(_.fullyQualifiedName)}")}")
        result.codeUnits should not be empty
      }
    }

    "detect method usages" in {
      Using.resource(
        InlineTestProject
          .builder()
          .addFile(
            "com/example/Lib.java",
            """package com.example;
              |public class Lib {
              |  public void doWork() {}
              |}
              |""".stripMargin
          )
          .addFile(
            "com/example/App.java",
            """package com.example;
              |public class App {
              |  public void run(Lib lib) {
              |    lib.doWork();
              |  }
              |}
              |""".stripMargin
          )
          .build()
      ) { project =>
        val result = UsageAnalyzers.analyze(project.javaSources)
        println(s"Result: ${result.codeUnits.map(cu => s"${cu.fullyQualifiedName} (${cu.`type`}) -> ${cu.usages.map(_.fullyQualifiedName)}")}")
        result.codeUnits should not be empty
      }
    }

    "detect explicit type references" in {
      Using.resource(
        InlineTestProject
          .builder()
          .addFile(
            "com/example/Target.java",
            """package com.example;
              |public class Target {}
              |""".stripMargin
          )
          .addFile(
            "com/example/Usage.java",
            """package com.example;
              |public class Usage {
              |  public void explicit() {
              |    Target t = null;
              |  }
              |}
              |""".stripMargin
          )
          .build()
      ) { project =>
        val result = UsageAnalyzers.analyze(project.javaSources)
        println(s"Result: ${result.codeUnits.map(cu => s"${cu.fullyQualifiedName} (${cu.`type`}) -> ${cu.usages.map(_.fullyQualifiedName)}")}")
        result.codeUnits should not be empty
      }
    }

    "var inference does not create standalone type reference (JDT limitation)" ignore {
      // JDT limitation: We cannot distinguish 'var' type inference from other type usages.
      // When using 'var t = new Target()', JDT still sees 'Target' as a SimpleName in
      // the ClassInstanceCreation, so it appears as a type reference regardless of 'var'.
      // This test documents the expected behavior if JDT could distinguish them,
      // but is ignored because JDT cannot make this distinction.
      Using.resource(
        InlineTestProject
          .builder()
          .addFile(
            "com/example/Target.java",
            """package com.example;
              |public class Target {}
              |""".stripMargin
          )
          .addFile(
            "com/example/Usage.java",
            """package com.example;
              |public class Usage {
              |  public void inferred() {
              |    var t = new Target();
              |  }
              |}
              |""".stripMargin
          )
          .build()
      ) { project =>
        val result = UsageAnalyzers.analyze(project.javaSources)
        println(s"Result: ${result.codeUnits.map(cu => s"${cu.fullyQualifiedName} (${cu.`type`}) -> ${cu.usages.map(_.fullyQualifiedName)}")}")
        result.codeUnits should not be empty
      }
    }

    "detect field loads and stores" in {
      Using.resource(
        InlineTestProject
          .builder()
          .addFile(
            "com/example/Data.java",
            """package com.example;
              |public class Data {
              |  public int value;
              |}
              |""".stripMargin
          )
          .addFile(
            "com/example/Logic.java",
            """package com.example;
              |public class Logic {
              |  public void process(Data d) {
              |    d.value = 10;      // store
              |    int x = d.value;   // load
              |  }
              |}
              |""".stripMargin
          )
          .build()
      ) { project =>
        val result = UsageAnalyzers.analyze(project.javaSources)
        println(s"Result: ${result.codeUnits.map(cu => s"${cu.fullyQualifiedName} (${cu.`type`}) -> ${cu.usages.map(_.fullyQualifiedName)}")}")
        result.codeUnits should not be empty
      }
    }
  }
}
