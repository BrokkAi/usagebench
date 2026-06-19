namespace Example;

public static class Consumer
{
    public static string Run()
    {
        var repository = new Repository();
        Service service = new Service(repository);
        var result = service.Execute(" Ada ");
        return Defaults.Prefix + result + repository.Last;
    }
}
