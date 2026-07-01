using WorkerAlias = Example.Parity.ConsoleHandler;

namespace Example.Parity;

public interface IHandler
{
    string Handle(string name);
}

public sealed class ConsoleSink
{
    public string Last { get; private set; } = "";

    public void Record(string value)
    {
        Last = value;
    }
}

public sealed partial class EventRecord
{
    public string Name { get; }

    public EventRecord(string name)
    {
        Name = name;
    }
}

public sealed class ConsoleHandler : IHandler
{
    private readonly ConsoleSink sink;

    public ConsoleHandler(ConsoleSink sink)
    {
        this.sink = sink;
    }

    public string Handle(string name)
    {
        sink.Record(name);
        return name;
    }
}

public static class HandlerExtensions
{
    public static string Tag(this string value)
    {
        return $"tag:{value}";
    }
}

public static partial class HandlerFactory
{
    public static IHandler Create()
    {
        return new WorkerAlias(new ConsoleSink());
    }
}
