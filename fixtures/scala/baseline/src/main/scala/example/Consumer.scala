package example

object Consumer {
  def run(): String = {
    val repository = new Repository()
    val service = Service.build(repository)
    val result = service.execute(" Ada ")
    Defaults.Prefix + result + repository.last
  }
}
