package precision

object Tools:
  def choose(value: String): String = value

object Maker:
  def apply(value: String): String = value

object Extensions:
  extension (value: String)
    def decorate: String = value
