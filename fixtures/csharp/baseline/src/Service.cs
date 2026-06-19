namespace Example;

public static class Defaults
{
    public const string Prefix = "job";
}

public class Repository
{
    public string Last { get; private set; } = "";

    public string Save(string value)
    {
        Last = value.Trim();
        return Last;
    }
}

public class Service
{
    private readonly Repository repository;

    public Service(Repository repository)
    {
        this.repository = repository;
    }

    public string Execute(string name)
    {
        var stored = repository.Save(name);
        return $"{Defaults.Prefix}:{stored}";
    }
}
