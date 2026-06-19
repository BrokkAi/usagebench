package example

object Defaults {
  val Prefix = "job"
}

class Repository {
  var last: String = ""

  def save(value: String): String = {
    last = value.trim
    last
  }
}

class Service(repository: Repository) {
  def execute(name: String): String = {
    val stored = repository.save(name)
    s"${Defaults.Prefix}:$stored"
  }
}

object Service {
  def build(repository: Repository): Service = new Service(repository)
}
