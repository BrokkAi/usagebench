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

        val fooConstructor = result.codeUnits.find(_.fullyQualifiedName == "com.example.Foo.Foo")
          .getOrElse(fail("Foo.Foo constructor not found"))
        fooConstructor.usages.map(_.fullyQualifiedName) should contain ("com.example.Bar.main")

        val fooClass = result.codeUnits.find(_.fullyQualifiedName == "com.example.Foo")
          .getOrElse(fail("com.example.Foo class not found"))
        fooClass.usages.map(_.fullyQualifiedName) should contain ("com.example.Bar.main")
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

        val doWorkMethod = result.codeUnits.find(_.fullyQualifiedName == "com.example.Lib.doWork")
          .getOrElse(fail("Lib.doWork not found"))
        doWorkMethod.usages.map(_.fullyQualifiedName) should contain ("com.example.App.run")

        val libClass = result.codeUnits.find(_.fullyQualifiedName == "com.example.Lib")
          .getOrElse(fail("com.example.Lib class not found"))
        libClass.usages.map(_.fullyQualifiedName) should contain ("com.example.App.run")
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

        val targetClass = result.codeUnits.find(_.fullyQualifiedName == "com.example.Target")
          .getOrElse(fail("com.example.Target not found"))
        targetClass.usages.map(_.fullyQualifiedName) should contain ("com.example.Usage.explicit")
      }
    }

    "var inference does not create a type reference when the type name is not present" in {
      // This test documents expected JDT behavior:
      // When using 'var t = lib.createTarget()', the type 'Target' is purely inferred.
      // Because 'Target' does not appear in the source code of Usage.java, JDT's AST
      // correctly does not contain any nodes referencing Target. Consequently,
      // our analyzer (rightly) does not report a usage of Target from Usage.bar.
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
            "com/example/Lib.java",
            """package com.example;
              |public class Lib {
              |  public Target createTarget() { return new Target(); }
              |}
              |""".stripMargin
          )
          .addFile(
            "com/example/Usage.java",
            """package com.example;
              |public class Usage {
              |  public void bar(Lib lib) {
              |    var t = lib.createTarget();
              |  }
              |}
              |""".stripMargin
          )
          .build()
      ) { project =>
        val result = UsageAnalyzers.analyze(project.javaSources)

        val targetUnit = result.codeUnits.find(_.fullyQualifiedName == "com.example.Target")
          .getOrElse(fail(s"Target not found in ${result.codeUnits.map(_.fullyQualifiedName)}"))
        val libMethodUnit = result.codeUnits.find(_.fullyQualifiedName == "com.example.Lib.createTarget")
          .getOrElse(fail(s"Lib.createTarget not found in ${result.codeUnits.map(_.fullyQualifiedName)}"))

        // Target should NOT be used by Usage.bar because the type name is hidden by 'var'
        targetUnit.usages.map(_.fullyQualifiedName) should not contain "com.example.Usage.bar"

        // Lib.createTarget SHOULD be used by Usage.bar
        libMethodUnit.usages.map(_.fullyQualifiedName) should contain ("com.example.Usage.bar")
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

        val valueField = result.codeUnits.find(_.fullyQualifiedName == "com.example.Data.value")
          .getOrElse(fail("com.example.Data.value field not found"))

        val usages = valueField.usages.map(_.fullyQualifiedName)
        usages.filter(_ == "com.example.Logic.process") should have size 2
      }
    }
  }
}
