package example

trait Renderer:
  def render(value: String): String

class ConsoleRenderer extends Renderer:
  override def render(value: String): String =
    value.trim

object ConsoleRenderer:
  def default: ConsoleRenderer =
    new ConsoleRenderer

class Workflow(renderer: Renderer):
  def run(value: String): String =
    renderer.render(value)

object Syntax:
  extension (value: String)
    def slug: String =
      value.toLowerCase.replace(" ", "-")

object App:
  import ConsoleRenderer.{default => renderer}
  import Syntax.*

  val workflow = new Workflow(renderer)
  val output = workflow.run("Hello World")
  val direct = renderer.render("  ok ")
  val slugged = "Hello World".slug

case class RenderRequest(value: String)

object SyntheticApp:
  val request = RenderRequest("Hello World")
  val copied = request.copy(value = "Goodbye")
  val observed = copied.value
