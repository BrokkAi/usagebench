namespace Example.Parity;

public sealed partial class EventRecord
{
    public string Label()
    {
        return Name.Tag();
    }
}

public static class ParityConsumer
{
    public static string Run()
    {
        IHandler handler = HandlerFactory.Create();
        var first = handler.Handle("Ada").Tag();
        var concrete = new ConsoleHandler(new ConsoleSink());
        var second = concrete.Handle("Ben");
        var record = new EventRecord(second);
        return first + record.Name + record.Label();
    }
}

public partial class GeneratedConsumer
{
    public partial string GeneratedName();
}
